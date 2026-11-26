//! `stencilator`: the command line.
//!
//! `stencilator build` fits a tile grid into a canvas, reduces the colours, and writes the
//! chart and one stencil per colour. The engine computes; this program reads the image
//! and writes the files, all or nothing.

mod files;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use serde_json::json;
use stencilator_core::chart::{ChartSettings, TileChart, build_chart};
use stencilator_core::colour::Rgb;
use stencilator_core::export::{ChartFormat, encode_chart};
use stencilator_core::quantise::ColourMode;
use stencilator_core::stencil::{
    RECOMMENDED_MIN_WEB_MM, check_thickness, encode_stl, stencil_filename, stencil_triangles,
};
use stencilator_core::{VERSION, source};

#[derive(Parser)]
#[command(name = "stencilator", version = VERSION, about = "Turns images into painting charts and 3D-printable stencils.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fit a tile grid to an image, reduce its colours, and write the chart and stencils.
    ///
    /// Without --chart, --tiles or --stencils it only prints what it would make.
    Build(Build),
}

#[derive(Args)]
struct Build {
    /// PNG, JPEG or WebP image.
    image: PathBuf,

    /// Canvas width and height in millimetres; the grid never exceeds it.
    #[arg(long, value_name = "WxH", default_value = "200x200", value_parser = parse_size)]
    canvas: (f64, f64),
    /// Side of a tile in millimetres.
    #[arg(long, value_name = "MM", default_value_t = 5.0)]
    tile: f64,
    /// Width of the grid lines in millimetres; 0 for none.
    #[arg(long, value_name = "MM", default_value_t = 0.8)]
    line: f64,
    /// Resolution of the chart; sizes are rounded to whole pixels at this DPI.
    #[arg(long, default_value_t = 300)]
    dpi: u32,
    /// colour, greyscale or black-white.
    #[arg(long, default_value = "colour", value_parser = parse_mode)]
    mode: ColourMode,
    /// Most colours in the palette (black-white always has two).
    #[arg(long, value_name = "N", default_value_t = 16)]
    colours: usize,
    /// Colour that transparent parts of the image take.
    #[arg(long, value_name = "RRGGBB", default_value = "ffffff", value_parser = parse_colour)]
    background: Rgb,
    /// Colour of the grid lines on the chart.
    #[arg(long, value_name = "RRGGBB", default_value = "000000", value_parser = parse_colour)]
    grid_colour: Rgb,
    /// Seed for the colour clustering; the same seed gives the same palette.
    #[arg(long, default_value_t = 0)]
    seed: u64,

    /// Write the chart with grid lines: .png, .jpg or .pdf.
    #[arg(long, value_name = "FILE")]
    chart: Option<PathBuf>,
    /// Write the tiles packed edge to edge: .png, .jpg or .pdf.
    #[arg(long, value_name = "FILE")]
    tiles: Option<PathBuf>,
    /// Write one STL stencil per colour into this folder.
    #[arg(long, value_name = "DIR")]
    stencils: Option<PathBuf>,
    /// Stencil thickness in millimetres.
    #[arg(long, value_name = "MM", default_value_t = 1.2)]
    thickness: f64,

    /// Print the summary as JSON.
    #[arg(long)]
    json: bool,
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

fn parse_mode(text: &str) -> Result<ColourMode, String> {
    text.parse().map_err(|e: stencilator_core::Error| e.to_string())
}

fn hex(colour: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
}

fn build(args: &Build) -> Result<(), String> {
    let settings = ChartSettings {
        canvas_mm: args.canvas,
        tile_mm: args.tile,
        line_mm: args.line,
        dpi: args.dpi,
        mode: args.mode,
        n_colours: args.colours,
        background: args.background,
        seed: args.seed,
    };
    // Settings first, so that a typing mistake is reported before a large image is read.
    settings.check().map_err(|e| e.to_string())?;
    if args.stencils.is_some() {
        check_thickness(args.thickness).map_err(|e| e.to_string())?;
    }
    let bytes = std::fs::read(&args.image).map_err(|e| format!("{}: {e}", args.image.display()))?;
    let image = source::decode(&bytes).map_err(|e| e.to_string())?;
    let chart = build_chart(&image, &settings).map_err(|e| e.to_string())?;

    let mut destinations: Vec<(String, PathBuf)> = Vec::new();
    for (flag, path) in [("--chart", &args.chart), ("--tiles", &args.tiles)] {
        if let Some(path) = path {
            destinations.push((flag.to_owned(), path.clone()));
        }
    }
    if let Some(dir) = &args.stencils {
        for &colour in &chart.palette {
            destinations.push((format!("the stencil for {}", hex(colour)), dir.join(stencil_filename(colour))));
        }
    }
    files::check_destinations(&destinations, &args.image)?;

    // Everything is computed before anything is written, so an error leaves no files.
    let mut outputs: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for (path, with_lines) in [(&args.chart, true), (&args.tiles, false)] {
        if let Some(path) = path {
            let format = ChartFormat::from_file_name(&path.to_string_lossy()).map_err(|e| e.to_string())?;
            outputs.push((
                path.clone(),
                encode_chart(&chart, format, args.grid_colour, with_lines).map_err(|e| e.to_string())?,
            ));
        }
    }
    let mut warnings = Vec::new();
    let mut created = Vec::new();
    if let Some(dir) = &args.stencils {
        if chart.layout.line_px > 0 && chart.layout.line_mm() < RECOMMENDED_MIN_WEB_MM {
            warnings.push(format!(
                "grid lines of {:.2} mm make a fragile stencil web; {RECOMMENDED_MIN_WEB_MM} mm or more suits a 0.4 mm nozzle",
                chart.layout.line_mm()
            ));
        }
        for (index, &colour) in (0..).zip(&chart.palette) {
            let holes: Vec<bool> = chart.labels.iter().map(|&l| l == index).collect();
            let triangles = stencil_triangles(&holes, &chart.layout, args.thickness).map_err(|e| e.to_string())?;
            let [r, g, b] = colour;
            let stl = encode_stl(&triangles, &format!("stencil rgb({r}, {g}, {b})"));
            outputs.push((dir.join(stencil_filename(colour)), stl));
        }
        created = files::create_dirs(dir)?;
    }
    if let Err(message) = files::write_all(&outputs) {
        files::remove_dirs(&created);
        return Err(message);
    }

    let written: Vec<&Path> = outputs.iter().map(|(p, _)| p.as_path()).collect();
    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary_json(&chart, &written, &warnings)).unwrap_or_default());
    } else {
        print_summary(&chart, &written, &warnings);
    }
    Ok(())
}

fn summary_json(chart: &TileChart, written: &[&Path], warnings: &[String]) -> serde_json::Value {
    let g = &chart.layout;
    let palette: Vec<_> =
        chart.palette.iter().zip(chart.tile_counts()).map(|(&c, n)| json!({ "colour": hex(c), "tiles": n })).collect();
    json!({
        "tiles": [g.tiles_x, g.tiles_y],
        "size_mm": [g.width_mm(), g.height_mm()],
        "size_px": [g.width_px(), g.height_px()],
        "tile_mm": g.tile_mm(),
        "line_mm": g.line_mm(),
        "dpi": g.dpi,
        "palette": palette,
        "written": written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "warnings": warnings,
    })
}

fn print_summary(chart: &TileChart, written: &[&Path], warnings: &[String]) {
    let g = &chart.layout;
    println!("grid     {} x {} tiles ({})", g.tiles_x, g.tiles_y, g.tile_count());
    println!(
        "size     {:.1} x {:.1} mm, {} x {} px at {} DPI",
        g.width_mm(),
        g.height_mm(),
        g.width_px(),
        g.height_px(),
        g.dpi
    );
    println!("tile     {:.3} mm, line {:.3} mm", g.tile_mm(), g.line_mm());
    println!("colours  {}", chart.palette.len());
    for (i, (&colour, count)) in chart.palette.iter().zip(chart.tile_counts()).enumerate() {
        println!("  {:>2}  {}  {count} tiles", i + 1, hex(colour));
    }
    for path in written {
        println!("wrote    {}", path.display());
    }
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Build(args) => build(args),
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
        assert!(parse_colour("ff88").is_err());
        assert_eq!(parse_mode("black-white").unwrap(), ColourMode::BlackWhite);
    }

    #[test]
    fn the_command_line_is_well_formed() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
