//! `stencilator freeform`: one plate per paint, with openings that follow the artwork, and
//! a report of what may not print well.

use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::{Value, json};
use stencilator_core::artwork::{LabelMap, label_drawing, label_picture};
use stencilator_core::colour::Rgb;
use stencilator_core::drawing::{parse_svg, text_drawing};
use stencilator_core::plate::{
    Plate, PlateReport, Spot, build_plates, check_plate, plate_files, plates_3mf, reports_json,
};
use stencilator_core::project::{ArtworkSize, Colour, Mode, Project, Source};
use stencilator_core::{source, zip};

use crate::sources::{Input, embed_fonts, font_data};
use crate::{ColourArgs, StencilArgs, files, hex, is_zip, parse_colour, parse_size, stencil_destinations};

/// Positions listed in a warning before the rest are counted.
const LISTED: usize = 3;

/// Below this share of a paint's area traced, detail is being lost.
const TRACED_ENOUGH: f64 = 0.95;

#[derive(Args)]
pub(crate) struct FreeformArgs {
    /// A PNG, JPEG, WebP or SVG file, or a project saved with --save.
    #[arg(required_unless_present = "text", conflicts_with = "text")]
    input: Option<PathBuf>,

    /// Cut this text instead of a file; repeat for more lines, which are centred.
    #[arg(long, value_name = "TEXT", help_heading = "Text")]
    text: Vec<String>,
    /// Size of the font in millimetres; capitals are about 0.7 of it [default: 30]
    #[arg(long, value_name = "MM", help_heading = "Text")]
    font_size: Option<f64>,
    /// Colour of the text [default: 000000]
    #[arg(long, value_name = "RRGGBB", value_parser = parse_colour, help_heading = "Text")]
    text_colour: Option<Rgb>,
    /// A TrueType or OpenType file, or a built-in font: "Noto Sans Bold", the default, or
    /// "Allerta Stencil", whose letters have bridges so their middles stay put. It sets
    /// typed text and stands in for any font a drawing names but does not carry; repeat
    /// for more. Letters a font lacks come from Noto Sans Bold.
    #[arg(long, value_name = "FILE|NAME", help_heading = "Text")]
    font: Vec<PathBuf>,

    /// Make the artwork this wide in millimetres; the height follows [default for
    /// pictures: 200]
    #[arg(long, value_name = "MM", conflicts_with_all = ["height", "within", "natural"], help_heading = "Size")]
    width: Option<f64>,
    /// Make the artwork this tall in millimetres; the width follows.
    #[arg(long, value_name = "MM", conflicts_with_all = ["within", "natural"], help_heading = "Size")]
    height: Option<f64>,
    /// Make the artwork as large as fits a box this wide and tall, in millimetres.
    #[arg(long, value_name = "WxH", value_parser = parse_size, conflicts_with = "natural", help_heading = "Size")]
    within: Option<(f64, f64)>,
    /// Keep the artwork's own size: an SVG's width and height, text at its font size. The
    /// default for both.
    #[arg(long, help_heading = "Size")]
    natural: bool,
    /// Side of a cell in millimetres, the finest detail the plates follow [default: 0.1]
    #[arg(long, value_name = "MM", help_heading = "Size")]
    cell: Option<f64>,
    /// Patches of one paint smaller than this, in square millimetres, take the paints
    /// around them; 0 keeps every patch [default: 1]
    #[arg(long, value_name = "MM2", help_heading = "Size")]
    min_patch: Option<f64>,

    /// Solid border around the artwork in millimetres [default: 10]
    #[arg(long, value_name = "MM", help_heading = "Plates")]
    margin: Option<f64>,
    /// Report plate material narrower than this, in millimetres [default: 0.6]
    #[arg(long, value_name = "MM", help_heading = "Plates")]
    min_web: Option<f64>,
    /// Report openings narrower than this, in millimetres [default: 0.5]
    #[arg(long, value_name = "MM", help_heading = "Plates")]
    min_opening: Option<f64>,

    #[command(flatten)]
    colours: ColourArgs,
    #[command(flatten)]
    stencil_settings: StencilArgs,

    /// Write one STL plate per paint into this folder, or into one zip if the name ends in
    /// .zip.
    #[arg(long, value_name = "DIR|ZIP", help_heading = "Output")]
    stencils: Option<PathBuf>,
    /// Write every plate into one 3MF file, side by side, each named and in its paint's
    /// colour.
    #[arg(long = "3mf", value_name = "FILE", help_heading = "Output")]
    three_mf: Option<PathBuf>,
    /// Save the project, with the artwork and every setting, to open again later.
    #[arg(long, value_name = "FILE.stencilator", help_heading = "Output")]
    save: Option<PathBuf>,
    /// Print the report as JSON. Places are in millimetres from a plate's bottom-left
    /// corner, as everywhere in the report.
    #[arg(long, help_heading = "Output")]
    json: bool,
}

impl FreeformArgs {
    fn apply(&self, project: &mut Project) -> Result<(), String> {
        project.mode = Mode::Freeform;
        if let Source::Text { font_mm, colour, .. } = &mut project.source {
            if let Some(size) = self.font_size {
                *font_mm = size;
            }
            if let Some(text_colour) = self.text_colour {
                *colour = Colour(text_colour);
            }
        } else if self.font_size.is_some() || self.text_colour.is_some() {
            return Err("--font-size and --text-colour are for typed text".to_owned());
        }
        if !self.font.is_empty() {
            project.fonts = embed_fonts(&self.font)?;
        }
        let f = &mut project.freeform;
        if let Some(size) = match (self.width, self.height, self.within) {
            (Some(w), _, _) => Some(ArtworkSize::WidthMm(w)),
            (_, Some(h), _) => Some(ArtworkSize::HeightMm(h)),
            (_, _, Some((w, h))) => Some(ArtworkSize::WithinMm([w, h])),
            _ if self.natural => Some(ArtworkSize::Natural),
            _ => None,
        } {
            f.size = size;
        }
        for (value, field) in [
            (self.cell, &mut f.cell_mm),
            (self.min_patch, &mut f.min_patch_mm2),
            (self.margin, &mut f.margin_mm),
            (self.min_web, &mut f.min_web_mm),
            (self.min_opening, &mut f.min_opening_mm),
        ] {
            if let Some(value) = value {
                *field = value;
            }
        }
        self.colours.apply(project);
        self.stencil_settings.apply(project);
        Ok(())
    }
}

pub(crate) fn run(args: &FreeformArgs) -> Result<(), String> {
    let input = Input::new(args.input.as_ref(), &args.text);
    let mut project = input.project()?;
    args.apply(&mut project)?;
    // Settings first, so that a typing mistake is reported before a large file is read.
    let artwork_settings = project.artwork_settings();
    let plate_settings = project.plate_settings();
    artwork_settings.check().map_err(|e| e.to_string())?;
    plate_settings.check().map_err(|e| e.to_string())?;
    input.load(&mut project)?;

    let map = label(&project)?.merged(&project.merges());
    let skip = project.stencil_skip();
    let plates = build_plates(&map, &plate_settings, &skip).map_err(|e| e.to_string())?;
    let reports = plates
        .iter()
        .map(|plate| check_plate(plate, &plate_settings))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut destinations: Vec<(String, PathBuf)> = Vec::new();
    for (flag, path) in [("--3mf", &args.three_mf), ("--save", &args.save)] {
        if let Some(path) = path {
            destinations.push((flag.to_owned(), path.clone()));
        }
    }
    if let Some(target) = &args.stencils {
        let names: Vec<(String, Rgb)> = plates.iter().map(|p| (p.file_name(), p.colour)).collect();
        destinations.extend(stencil_destinations(target, &names));
    }
    files::check_destinations(&destinations, &input)?;

    // Everything is computed before anything is written, so an error leaves no files.
    let thickness = project.stencils.thickness_mm;
    let mut outputs: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    if let Some(path) = &args.three_mf {
        outputs.push((path.clone(), plates_3mf(&plates, thickness).map_err(|e| e.to_string())?));
    }
    if let Some(path) = &args.save {
        outputs.push((path.clone(), project.to_json()));
    }
    let mut created = Vec::new();
    if let Some(target) = &args.stencils {
        let stls = plate_files(&plates, thickness).map_err(|e| e.to_string())?;
        if is_zip(target) {
            outputs.push((target.clone(), zip::zip(&stls).map_err(|e| e.to_string())?));
        } else {
            outputs.extend(stls.into_iter().map(|(name, stl)| (target.join(name), stl)));
            created = files::create_dirs(target)?;
        }
    }
    if let Err(message) = files::write_all(&outputs) {
        files::remove_dirs(&created);
        return Err(message);
    }

    let written: Vec<&Path> = outputs.iter().map(|(p, _)| p.as_path()).collect();
    let report =
        Report { project: &project, map: &map, skip: &skip, plates: &plates, reports: &reports, written: &written };
    let warnings = report.warnings();
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report.json(&warnings)).unwrap_or_default());
    } else {
        report.print();
        for warning in &warnings {
            eprintln!("warning: {warning}");
        }
    }
    Ok(())
}

/// The label map of the project's artwork.
fn label(project: &Project) -> Result<LabelMap, String> {
    let settings = project.artwork_settings();
    let map = match &project.source {
        Source::Picture { data, .. } => label_picture(&source::decode(data).map_err(|e| e.to_string())?, &settings),
        Source::Svg { data, .. } => {
            label_drawing(&parse_svg(data, &font_data(project)?).map_err(|e| e.to_string())?, &settings)
        }
        Source::Text { text, font_mm, colour } => label_drawing(
            &text_drawing(text, &font_data(project)?, *font_mm, colour.0).map_err(|e| e.to_string())?,
            &settings,
        ),
    };
    map.map_err(|e| e.to_string())
}

struct Report<'a> {
    project: &'a Project,
    map: &'a LabelMap,
    skip: &'a [Rgb],
    plates: &'a [Plate],
    reports: &'a [PlateReport],
    written: &'a [&'a Path],
}

/// Millimetres to the micrometre, which is all the engine resolves.
fn um(mm: f64) -> f64 {
    (mm * 1000.0).round() / 1000.0
}

/// "x 12.3, y 45.6": the middle of a spot.
fn place(spot: &Spot) -> String {
    format!("x {:.1}, y {:.1}", spot.x_mm + spot.width_mm / 2.0, spot.y_mm + spot.height_mm / 2.0)
}

/// The first few places, and how many more.
fn places(spots: &[Spot]) -> String {
    let listed: Vec<String> = spots.iter().take(LISTED).map(place).collect();
    let more = spots.len().saturating_sub(LISTED);
    let text = format!("at {}", listed.join("; "));
    if more > 0 { format!("{text} and {more} more") } else { text }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Report<'_> {
    fn cell_mm2(&self) -> f64 {
        let cell = f64::from(self.map.cell_um) / 1000.0;
        cell * cell
    }

    fn plate_name(report: &PlateReport) -> String {
        format!("plate {} ({})", report.paint + 1, hex(report.colour))
    }

    fn warnings(&self) -> Vec<String> {
        let settings = self.project.plate_settings();
        let mut warnings = Vec::new();
        for report in self.reports {
            let name = Self::plate_name(report);
            if !report.islands.is_empty() {
                let largest = report.islands.iter().max_by(|a, b| a.area_mm2.total_cmp(&b.area_mm2));
                let largest = largest.map_or(String::new(), |s| {
                    format!("; the largest, {:.1} x {:.1} mm, is at {}", s.width_mm, s.height_mm, place(s))
                });
                warnings.push(format!(
                    "{name}: {} fall out unless something holds them{largest}",
                    count(report.islands.len(), "island", "islands")
                ));
            }
            if !report.thin_webs.is_empty() {
                warnings.push(format!(
                    "{name}: {} thinner than {} mm may break, {}",
                    count(report.thin_webs.len(), "web", "webs"),
                    settings.min_web_mm,
                    places(&report.thin_webs)
                ));
            }
            if !report.narrow_openings.is_empty() {
                warnings.push(format!(
                    "{name}: {} narrower than {} mm may close up in printing, {}",
                    count(report.narrow_openings.len(), "opening", "openings"),
                    settings.min_opening_mm,
                    places(&report.narrow_openings)
                ));
            }
            if report.paint_mm2 > 0.0 && report.opening_mm2 < TRACED_ENOUGH * report.paint_mm2 {
                warnings.push(format!(
                    "{name}: only {:.0} % of the paint's area could be traced; detail finer than the {} mm cells is lost",
                    100.0 * report.opening_mm2 / report.paint_mm2,
                    f64::from(self.map.cell_um) / 1000.0
                ));
            }
        }
        if let Some(report) = self.reports.first().filter(|r| !r.fits_bed) {
            let bed = self.project.stencils.bed_mm;
            warnings.push(format!(
                "the plates, {:.1} x {:.1} mm, do not fit a {} x {} mm bed",
                report.width_mm, report.height_mm, bed[0], bed[1]
            ));
        }
        warnings
    }

    fn plate_file(&self, paint: usize) -> Option<String> {
        self.plates.iter().find(|p| p.paint == paint).map(Plate::file_name)
    }

    fn json(&self, warnings: &[String]) -> Value {
        let map = self.map;
        let palette: Vec<Value> = map
            .palette
            .iter()
            .zip(map.cell_counts())
            .enumerate()
            .map(|(paint, (&colour, cells))| {
                #[allow(clippy::cast_precision_loss, reason = "at most 16 million cells")]
                let area = cells as f64 * self.cell_mm2();
                json!({ "colour": hex(colour), "cells": cells, "area_mm2": um(area), "stencil": self.plate_file(paint) })
            })
            .collect();
        json!({
            "mode": "freeform",
            "size_mm": [um(map.width_mm()), um(map.height_mm())],
            "cells": [map.width, map.height],
            "cell_mm": f64::from(map.cell_um) / 1000.0,
            "thickness_mm": self.project.stencils.thickness_mm,
            "palette": palette,
            "plates": reports_json(self.plates, self.reports),
            "written": self.written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "warnings": warnings,
        })
    }

    fn print(&self) {
        let map = self.map;
        println!(
            "artwork  {:.1} x {:.1} mm, {} x {} cells of {} mm",
            map.width_mm(),
            map.height_mm(),
            map.width,
            map.height,
            f64::from(map.cell_um) / 1000.0
        );
        println!("paints   {}", map.palette.len());
        for (paint, (&colour, cells)) in map.palette.iter().zip(map.cell_counts()).enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "at most 16 million cells")]
            let area = cells as f64 * self.cell_mm2();
            let stencil = self
                .plate_file(paint)
                .unwrap_or_else(|| if self.skip.contains(&colour) { "no stencil".to_owned() } else { String::new() });
            println!("  {:>2}  {}  {area:>9.1} mm²  {stencil}", paint + 1, hex(colour));
        }
        if let Some(first) = self.reports.first() {
            println!(
                "plates   {:.1} x {:.1} mm, {} mm thick",
                first.width_mm, first.height_mm, self.project.stencils.thickness_mm
            );
        }
        for r in self.reports {
            println!(
                "  {:>2}  {}  {}, {}, {}, {}",
                r.paint + 1,
                hex(r.colour),
                count(r.openings, "opening", "openings"),
                count(r.islands.len(), "island", "islands"),
                count(r.thin_webs.len(), "thin web", "thin webs"),
                count(r.narrow_openings.len(), "narrow opening", "narrow openings"),
            );
        }
        for path in self.written {
            println!("wrote    {}", path.display());
        }
    }
}
