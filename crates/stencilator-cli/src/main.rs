//! `stencilator`: the command line.
//!
//! `stencilator tiles` lays a grid of square tiles over a picture and writes a chart to
//! paint from and one stencil per colour. `stencilator freeform` makes plates whose
//! openings follow the shapes of a picture, an SVG drawing or typed text, and reports what
//! may not print well. Both start from a project, a new one or one saved earlier, change
//! what the options say, and can save the result. The engine computes; this program reads
//! and writes the files, all or nothing.

mod files;
mod freeform;
mod sources;
mod tiles;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use stencilator_core::VERSION;
use stencilator_core::colour::Rgb;
use stencilator_core::project::{Colour, Project};
use stencilator_core::quantise::ColourMode;

#[derive(Parser)]
#[command(name = "stencilator", version = VERSION, about = "Turns artwork into painting charts and 3D-printable stencils.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lay a grid of square tiles over a picture, reduce its colours, and write the chart
    /// and one stencil per colour.
    ///
    /// Without --chart, --tiles, --stencils or --save it only prints what it would make.
    /// Settings not given come from the project, if the input is one, or are the defaults.
    Tiles(tiles::TilesArgs),
    /// Make one plate per paint, with openings that follow the shapes of a picture, an SVG
    /// drawing or typed text, and report what may not print well.
    ///
    /// Without --stencils, --3mf or --save it only prints what it would make. Settings not
    /// given come from the project, if the input is one, or are the defaults.
    Freeform(freeform::FreeformArgs),
}

/// How colours become paints, in both modes.
#[derive(Args)]
#[command(next_help_heading = "Colours")]
pub(crate) struct ColourArgs {
    /// colour, greyscale or black-white [default: colour]
    #[arg(long, value_parser = parse_mode)]
    mode: Option<ColourMode>,
    /// Most paints; black-white always has two [default: 16]
    #[arg(long, value_name = "N")]
    colours: Option<usize>,
    /// Colour of the canvas, which transparent parts of the artwork take [default: ffffff]
    #[arg(long, value_name = "RRGGBB", value_parser = parse_colour)]
    background: Option<Rgb>,
    /// Seed for the colour clustering; the same seed gives the same paints [default: 0]
    #[arg(long)]
    seed: Option<u64>,
}

impl ColourArgs {
    fn apply(&self, project: &mut Project) {
        let colours = &mut project.colours;
        if let Some(mode) = self.mode {
            colours.mode = mode.into();
        }
        if let Some(count) = self.colours {
            colours.count = count;
        }
        if let Some(background) = self.background {
            colours.background = Colour(background);
        }
        if let Some(seed) = self.seed {
            colours.seed = seed;
        }
    }
}

/// Stencils and the choices made about paints, in both modes.
#[derive(Args)]
#[command(next_help_heading = "Stencils")]
pub(crate) struct StencilArgs {
    /// Merge one paint into another once the colours are reduced; repeat for more, in
    /// order. Colours not among the paints are ignored.
    #[arg(long, value_name = "RRGGBB=RRGGBB", value_parser = parse_merge)]
    merge: Vec<(Rgb, Rgb)>,
    /// Make no stencil for this paint; repeat for more.
    #[arg(long, value_name = "RRGGBB", value_parser = parse_colour)]
    skip: Vec<Rgb>,
    /// Make a stencil for the canvas colour too; it rarely needs one, so it has none
    /// unless asked for.
    #[arg(long)]
    stencil_canvas_colour: bool,
    /// Thickness in millimetres [default: 1.2]
    #[arg(long, value_name = "MM")]
    thickness: Option<f64>,
    /// Width and depth of the print bed in millimetres; larger stencils are reported
    /// [default: 256x256]
    #[arg(long, value_name = "WxH", value_parser = parse_size)]
    bed: Option<(f64, f64)>,
}

impl StencilArgs {
    fn apply(&self, project: &mut Project) {
        let stencils = &mut project.stencils;
        stencils.merges.extend(self.merge.iter().map(|&(from, to)| [Colour(from), Colour(to)]));
        stencils.skipped.extend(self.skip.iter().map(|&c| Colour(c)));
        stencils.stencil_canvas_colour |= self.stencil_canvas_colour;
        if let Some(thickness) = self.thickness {
            stencils.thickness_mm = thickness;
        }
        if let Some((w, h)) = self.bed {
            stencils.bed_mm = [w, h];
        }
    }
}

fn parse_size(text: &str) -> Result<(f64, f64), String> {
    let (w, h) = text.split_once(['x', 'X', '×', ',']).ok_or("expected WIDTHxHEIGHT, such as 300x200")?;
    let number = |s: &str| s.trim().parse::<f64>().map_err(|e| format!("{s:?}: {e}"));
    Ok((number(w)?, number(h)?))
}

fn parse_colour(text: &str) -> Result<Rgb, String> {
    let hex = text.trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected six hex digits, such as ff8800".to_owned());
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| e.to_string());
    Ok([channel(0)?, channel(2)?, channel(4)?])
}

fn parse_merge(text: &str) -> Result<(Rgb, Rgb), String> {
    let (from, to) = text.split_once('=').ok_or("expected FROM=TO, such as 9c3d17=e8772e")?;
    Ok((parse_colour(from)?, parse_colour(to)?))
}

fn parse_mode(text: &str) -> Result<ColourMode, String> {
    text.parse().map_err(|e: stencilator_core::Error| e.to_string())
}

pub(crate) fn hex(colour: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
}

pub(crate) fn is_zip(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Where each stencil goes: one file per stencil in a folder, or one zip.
pub(crate) fn stencil_destinations(target: &Path, names: &[(String, Rgb)]) -> Vec<(String, PathBuf)> {
    if is_zip(target) {
        vec![("--stencils".to_owned(), target.to_path_buf())]
    } else {
        names.iter().map(|(name, colour)| (format!("the stencil for {}", hex(*colour)), target.join(name))).collect()
    }
}

/// Whether a stencil `size_mm` fits the bed, turned if need be.
pub(crate) fn fits_bed(size_mm: (f64, f64), bed_mm: [f64; 2]) -> bool {
    let (w, h) = size_mm;
    (w <= bed_mm[0] && h <= bed_mm[1]) || (w <= bed_mm[1] && h <= bed_mm[0])
}

/// The project as saved, in its mode.
pub(crate) fn project_file(project: &Project) -> Vec<u8> {
    let mut saved = project.clone();
    sources::name_default_font(&mut saved);
    saved.to_json()
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Tiles(args) => tiles::run(args),
        Command::Freeform(args) => freeform::run(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("stencilator: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_colours_parse() {
        assert_eq!(parse_size("300x200").unwrap(), (300.0, 200.0));
        assert_eq!(parse_size("297 × 210").unwrap(), (297.0, 210.0));
        assert!(parse_size("300").is_err());
        assert_eq!(parse_colour("#FF8800").unwrap(), [255, 136, 0]);
        assert_eq!(parse_merge("9c3d17=#E8772E").unwrap(), ([156, 61, 23], [232, 119, 46]));
        assert!(parse_merge("9c3d17").is_err());
        assert!(parse_colour("ff88").is_err());
        assert_eq!(parse_mode("black-white").unwrap(), ColourMode::BlackWhite);
        assert!(fits_bed((250.0, 120.0), [200.0, 256.0]) && !fits_bed((257.0, 10.0), [256.0, 256.0]));
    }

    #[test]
    fn the_command_line_is_well_formed() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
