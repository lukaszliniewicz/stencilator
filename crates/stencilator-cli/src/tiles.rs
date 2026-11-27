//! `stencilator tiles`: a grid of square tiles, a chart to paint from and one stencil per
//! colour.

use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::json;
use stencilator_core::chart::{Sizing, TileChart, build_chart, merge_paints};
use stencilator_core::colour::Rgb;
use stencilator_core::export::{ChartFormat, encode_chart};
use stencilator_core::project::{Colour, Mode, Project, Source};
use stencilator_core::stencil::{RECOMMENDED_MIN_WEB_MM, stencil_filename, stencil_files};
use stencilator_core::{source, zip};

use crate::sources::Input;
use crate::{ColourArgs, StencilArgs, files, fits_bed, hex, is_zip, parse_colour, parse_size, stencil_destinations};

#[derive(Args)]
pub(crate) struct TilesArgs {
    /// A PNG, JPEG or WebP picture, or a project saved with --save.
    input: PathBuf,

    /// Canvas width and height in millimetres; the grid never exceeds it [default: 200x200]
    #[arg(long, value_name = "WxH", value_parser = parse_size, help_heading = "Grid")]
    canvas: Option<(f64, f64)>,
    /// One tile per pixel, for pixel art, instead of fitting a canvas; the size follows
    /// from --tile and --line. Art scaled up by a whole factor counts at its drawn size.
    #[arg(long, conflicts_with = "canvas", help_heading = "Grid")]
    pixels: bool,
    /// Side of a tile in millimetres [default: 5]
    #[arg(long, value_name = "MM", help_heading = "Grid")]
    tile: Option<f64>,
    /// Width of the grid lines in millimetres; 0 for none [default: 0.8]
    #[arg(long, value_name = "MM", help_heading = "Grid")]
    line: Option<f64>,
    /// Resolution of the chart; sizes are rounded to whole pixels at it [default: 300]
    #[arg(long, help_heading = "Grid")]
    dpi: Option<u32>,
    /// Colour of the grid lines on the chart [default: 000000]
    #[arg(long, value_name = "RRGGBB", value_parser = parse_colour, help_heading = "Grid")]
    grid_colour: Option<Rgb>,

    #[command(flatten)]
    colours: ColourArgs,
    #[command(flatten)]
    stencil_settings: StencilArgs,

    /// Write the chart with grid lines: .png, .jpg or .pdf.
    #[arg(long, value_name = "FILE", help_heading = "Output")]
    chart: Option<PathBuf>,
    /// Write the tiles packed edge to edge: .png, .jpg or .pdf.
    #[arg(long, value_name = "FILE", help_heading = "Output")]
    tiles: Option<PathBuf>,
    /// Write one STL stencil per paint into this folder, or into one zip if the name ends
    /// in .zip.
    #[arg(long, value_name = "DIR|ZIP", help_heading = "Output")]
    stencils: Option<PathBuf>,
    /// Save the project, with the picture and every setting, to open again later.
    #[arg(long, value_name = "FILE.stencilator", help_heading = "Output")]
    save: Option<PathBuf>,
    /// Print the summary as JSON.
    #[arg(long, help_heading = "Output")]
    json: bool,
}

impl TilesArgs {
    fn apply(&self, project: &mut Project) {
        project.mode = Mode::Tiles;
        let tiles = &mut project.tiles;
        if let Some((w, h)) = self.canvas {
            tiles.canvas_mm = [w, h];
            tiles.sizing = Sizing::Canvas.into();
        }
        if self.pixels {
            tiles.sizing = Sizing::Pixels.into();
        }
        if let Some(tile) = self.tile {
            tiles.tile_mm = tile;
        }
        if let Some(line) = self.line {
            tiles.line_mm = line;
        }
        if let Some(dpi) = self.dpi {
            tiles.dpi = dpi;
        }
        if let Some(colour) = self.grid_colour {
            tiles.grid_colour = Colour(colour);
        }
        self.colours.apply(project);
        self.stencil_settings.apply(project);
    }
}

pub(crate) fn run(args: &TilesArgs) -> Result<(), String> {
    let input = Input::new(Some(&args.input), &[]);
    let mut project = input.project()?;
    args.apply(&mut project);
    // Settings first, so that a typing mistake is reported before a large picture is read.
    project.check().map_err(|e| e.to_string())?;
    picture(&project)?;
    input.load(&mut project)?;
    let chart = build(&project)?;
    let skip = project.stencil_skip();

    let mut destinations: Vec<(String, PathBuf)> = Vec::new();
    for (flag, path) in [("--chart", &args.chart), ("--tiles", &args.tiles), ("--save", &args.save)] {
        if let Some(path) = path {
            destinations.push((flag.to_owned(), path.clone()));
        }
    }
    if let Some(target) = &args.stencils {
        let count = chart.palette.len();
        let names: Vec<(String, Rgb)> = (0..)
            .zip(&chart.palette)
            .filter(|(_, c)| !skip.contains(c))
            .map(|(index, &colour)| (stencil_filename(index, count, colour), colour))
            .collect();
        destinations.extend(stencil_destinations(target, &names));
    }
    files::check_destinations(&destinations, &input)?;

    // Everything is computed before anything is written, so an error leaves no files.
    let mut outputs: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let grid_colour = project.tiles.grid_colour.0;
    for (path, with_lines) in [(&args.chart, true), (&args.tiles, false)] {
        if let Some(path) = path {
            let format = ChartFormat::from_file_name(&path.to_string_lossy()).map_err(|e| e.to_string())?;
            outputs.push((
                path.clone(),
                encode_chart(&chart, format, grid_colour, with_lines).map_err(|e| e.to_string())?,
            ));
        }
    }
    if let Some(path) = &args.save {
        outputs.push((path.clone(), project.to_json()));
    }
    let mut created = Vec::new();
    if let Some(target) = &args.stencils {
        let stencils = stencil_files(&chart, project.stencils.thickness_mm, &skip).map_err(|e| e.to_string())?;
        if is_zip(target) {
            outputs.push((target.clone(), zip::zip(&stencils).map_err(|e| e.to_string())?));
        } else {
            outputs.extend(stencils.into_iter().map(|(name, stl)| (target.join(name), stl)));
            created = files::create_dirs(target)?;
        }
    }
    if let Err(message) = files::write_all(&outputs) {
        files::remove_dirs(&created);
        return Err(message);
    }

    let written: Vec<&Path> = outputs.iter().map(|(p, _)| p.as_path()).collect();
    let summary = Summary::new(&chart, &project, &skip, &written);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary.json()).unwrap_or_default());
    } else {
        summary.print();
    }
    Ok(())
}

fn picture(project: &Project) -> Result<&[u8], String> {
    match &project.source {
        Source::Picture { data, .. } => Ok(data),
        _ => {
            Err("tiles take PNG, JPEG and WebP pictures; use stencilator freeform for SVG drawings and text".to_owned())
        }
    }
}

/// Builds the chart without reading or writing any files.
pub(crate) fn build(project: &Project) -> Result<TileChart, String> {
    let image = source::decode(picture(project)?).map_err(|e| e.to_string())?;
    let chart = build_chart(&image, &project.chart_settings()).map_err(|e| e.to_string())?;
    Ok(merge_paints(&chart, &project.merges()))
}

pub(crate) struct Summary<'a> {
    chart: &'a TileChart,
    skip: &'a [Rgb],
    written: &'a [&'a Path],
    warnings: Vec<String>,
    fits_bed: bool,
}

impl<'a> Summary<'a> {
    pub(crate) fn new(chart: &'a TileChart, project: &Project, skip: &'a [Rgb], written: &'a [&'a Path]) -> Self {
        let g = &chart.layout;
        let size_mm = (g.width_mm(), g.height_mm());
        let [bed_w, bed_h] = project.stencils.bed_mm;
        let fits_bed = fits_bed(size_mm, [bed_w, bed_h]);
        let mut warnings = Vec::new();
        if g.line_px > 0 && g.line_mm() < RECOMMENDED_MIN_WEB_MM {
            warnings.push(format!(
                "grid lines of {:.2} mm make a fragile stencil web; {RECOMMENDED_MIN_WEB_MM} mm or more suits a 0.4 mm nozzle",
                g.line_mm()
            ));
        }
        if !fits_bed {
            warnings.push(format!(
                "the stencils, {:.1} x {:.1} mm, do not fit a {bed_w} x {bed_h} mm bed",
                size_mm.0, size_mm.1
            ));
        }
        Self { chart, skip, written, warnings, fits_bed }
    }

    pub(crate) fn json(&self) -> serde_json::Value {
        let g = &self.chart.layout;
        let count = self.chart.palette.len();
        let palette: Vec<_> = (0..)
            .zip(self.chart.palette.iter().zip(self.chart.tile_counts()))
            .map(|(index, (&c, n))| {
                let stencil = (!self.skip.contains(&c)).then(|| stencil_filename(index, count, c));
                json!({ "colour": hex(c), "tiles": n, "stencil": stencil })
            })
            .collect();
        json!({
            "mode": "tiles",
            "tiles": [g.tiles_x, g.tiles_y],
            "size_mm": [g.width_mm(), g.height_mm()],
            "size_px": [g.width_px(), g.height_px()],
            "tile_mm": g.tile_mm(),
            "line_mm": g.line_mm(),
            "dpi": g.dpi,
            "fits_bed": self.fits_bed,
            "palette": palette,
            "written": self.written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "warnings": self.warnings,
        })
    }

    fn print(&self) {
        let g = &self.chart.layout;
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
        println!("colours  {}", self.chart.palette.len());
        for (i, (&colour, count)) in self.chart.palette.iter().zip(self.chart.tile_counts()).enumerate() {
            let note = if self.skip.contains(&colour) { ", no stencil" } else { "" };
            println!("  {:>2}  {}  {count} tiles{note}", i + 1, hex(colour));
        }
        for path in self.written {
            println!("wrote    {}", path.display());
        }
        for warning in &self.warnings {
            eprintln!("warning: {warning}");
        }
    }
}
