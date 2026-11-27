//! `stencilator freeform`: one plate per paint, with openings that follow the artwork, and
//! a report of what may not print well.

use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::{Value, json};
use stencilator_core::artwork::{LabelMap, label_drawing, label_picture};
use stencilator_core::colour::Rgb;
use stencilator_core::drawing::{parse_svg, text_drawing};
use stencilator_core::plate::{
    Mark, Plate, PlateReport, Spot, build_plates, check_plate, plate_files, plates_3mf, registration_frame,
    reports_json,
};
use stencilator_core::project::{ArtworkSize, Colour, Freeform, Mode, Project, Source};
use stencilator_core::registration::FrameReport;
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
    /// typed text or adds a font to a drawing. Repeat for more. A drawing keeps the
    /// fonts it carries unless given another file of the same name.
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
    /// Hold islands with flat bridges (1.2 mm by default). Pass =MM to set their width,
    /// or =0 to turn them off in a saved project.
    #[arg(
        long,
        value_name = "MM",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "1.2",
        conflicts_with = "raised_connectors",
        help_heading = "Plates"
    )]
    flat_bridges: Option<f64>,
    /// Trial: hold islands above the openings (1.2 mm wide). Pass =MM for a different
    /// width, or =0 to turn them off.
    #[arg(
        long, value_name = "MM", num_args = 0..=1, require_equals = true,
        default_missing_value = "1.2", help_heading = "Plates"
    )]
    raised_connectors: Option<f64>,
    /// Minimum height beneath a raised beam, from the work face [default: 4]
    #[arg(long, value_name = "MM", help_heading = "Plates")]
    connector_clearance: Option<f64>,
    /// Thickness of a raised beam [default: 1]
    #[arg(long, value_name = "MM", help_heading = "Plates")]
    beam_thickness: Option<f64>,
    /// Warn about free spans longer than this [default: 40]
    #[arg(long, value_name = "MM", help_heading = "Plates")]
    warn_span: Option<f64>,
    /// Key the plates to one registration frame (0.3 mm clearance at each edge).
    /// Pass =MM to set the clearance, or =0 for plain plates.
    #[arg(
        long,
        value_name = "MM",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "0.3",
        help_heading = "Plates"
    )]
    registration: Option<f64>,
    #[command(flatten)]
    finishing: FinishingArgs,

    #[command(flatten)]
    colours: ColourArgs,
    #[command(flatten)]
    stencil_settings: StencilArgs,

    /// Write one STL plate per paint and any registration frame into this folder,
    /// or into one zip if the name ends in .zip.
    #[arg(long, value_name = "DIR|ZIP", help_heading = "Output")]
    stencils: Option<PathBuf>,
    /// Write the plates and any registration frame side by side in one 3MF file.
    #[arg(long = "3mf", value_name = "FILE", help_heading = "Output")]
    three_mf: Option<PathBuf>,
    /// Write just the registration frame as STL. Requires registration to be enabled.
    #[arg(long, value_name = "FILE.stl", help_heading = "Output")]
    frame: Option<PathBuf>,
    /// Save the project, with the artwork and every setting, to open again later.
    #[arg(long, value_name = "FILE.stencilator", help_heading = "Output")]
    save: Option<PathBuf>,
    /// Print the report as JSON. Places are in millimetres from a plate's bottom-left
    /// corner, as everywhere in the report.
    #[arg(long, help_heading = "Output")]
    json: bool,
}

#[derive(Args)]
struct FinishingArgs {
    /// Widen openings on the spray side, in steps [default: 0]
    #[arg(long, value_name = "MM", help_heading = "Finishing")]
    chamfer: Option<f64>,
    /// Tuck earlier coats under later ones wherever their openings meet [default: 0]
    #[arg(long, value_name = "MM", help_heading = "Finishing")]
    overlap: Option<f64>,
    /// Deboss the margin [possible values: none, number, ral]
    #[arg(long, value_name = "MARK", value_parser = ["none", "number", "ral"], help_heading = "Finishing")]
    mark: Option<String>,
    /// Show approximate RAL Classic paint matches. Pass =false to turn these off.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true", help_heading = "Finishing")]
    match_ral: Option<bool>,
    /// Apply these paints first, in this order; others follow in palette order.
    #[arg(long, value_name = "RRGGBB,...", value_delimiter = ',', value_parser = parse_colour, help_heading = "Finishing")]
    paint_order: Vec<Rgb>,
    /// Clear the paint order in a saved project.
    #[arg(long, conflicts_with = "paint_order", help_heading = "Finishing")]
    palette_order: bool,
}

impl FinishingArgs {
    fn apply(&self, freeform: &mut Freeform) {
        let finish = &mut freeform.finish;
        finish.chamfer_mm = self.chamfer.unwrap_or(finish.chamfer_mm);
        finish.overlap_mm = self.overlap.unwrap_or(finish.overlap_mm);
        finish.match_ral = self.match_ral.unwrap_or(finish.match_ral);
        finish.mark = match self.mark.as_deref() {
            Some("number") => Mark::Number,
            Some("ral") => Mark::Ral,
            Some("none") => Mark::None,
            _ => finish.mark,
        };
        if self.palette_order || !self.paint_order.is_empty() {
            freeform.paint_order = self.paint_order.iter().copied().map(Colour).collect();
        }
    }
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
            let fonts = embed_fonts(&self.font)?;
            if matches!(project.source, Source::Svg { .. }) {
                for font in fonts {
                    if let Some(existing) = project.fonts.iter_mut().find(|existing| existing.name == font.name) {
                        *existing = font;
                    } else {
                        project.fonts.push(font);
                    }
                }
            } else {
                project.fonts = fonts;
            }
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
        for (value, field) in
            [(self.flat_bridges, &mut f.bridge_width_mm), (self.registration, &mut f.registration_clearance_mm)]
        {
            if let Some(value) = value {
                *field = (value != 0.0).then_some(value);
            }
        }
        self.apply_connectors(f)?;
        self.finishing.apply(f);
        self.colours.apply(project);
        self.stencil_settings.apply(project);
        Ok(())
    }

    fn apply_connectors(&self, f: &mut Freeform) -> Result<(), String> {
        if self.flat_bridges.is_some_and(|width| width != 0.0) {
            f.raised_connectors = None;
        }
        if let Some(width) = self.raised_connectors {
            if width == 0.0 {
                f.raised_connectors = None;
            } else {
                f.bridge_width_mm = None;
                f.raised_connectors.get_or_insert_default().width_mm = width;
            }
        }
        let dimensions = [self.connector_clearance, self.beam_thickness, self.warn_span];
        if dimensions.iter().any(Option::is_some) {
            let raised = f.raised_connectors.as_mut().ok_or("Add --raised-connectors to set beam dimensions.")?;
            raised.clearance_mm = self.connector_clearance.unwrap_or(raised.clearance_mm);
            raised.beam_mm = self.beam_thickness.unwrap_or(raised.beam_mm);
            raised.warn_mm = self.warn_span.unwrap_or(raised.warn_mm);
        }
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
    if args.frame.is_some() && plate_settings.registration_clearance_mm.is_none() {
        return Err("Add --registration to make a registration frame.".to_owned());
    }
    input.load(&mut project)?;

    let map = label(&project)?.merged(&project.merges());
    let skip = project.stencil_skip();
    let plates = build_plates(&map, &plate_settings, &skip).map_err(|e| e.to_string())?;
    if plates.is_empty() && (args.three_mf.is_some() || args.stencils.is_some() || args.frame.is_some()) {
        return Err("Every paint is skipped, so there are no plates to make.".to_owned());
    }
    let reports = plates
        .iter()
        .map(|plate| check_plate(plate, &plate_settings))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let frame = registration_frame(&plates);
    let frame_report = frame.as_ref().map(|frame| frame.report(plate_settings.bed_mm));

    let mut destinations: Vec<(String, PathBuf)> = Vec::new();
    for (flag, path) in [("--3mf", &args.three_mf), ("--frame", &args.frame), ("--save", &args.save)] {
        if let Some(path) = path {
            destinations.push((flag.to_owned(), path.clone()));
        }
    }
    if let Some(target) = &args.stencils {
        let names: Vec<(String, Rgb)> = plates.iter().map(|p| (p.file_name(), p.colour)).collect();
        destinations.extend(stencil_destinations(target, &names));
        if let Some(report) = frame_report.filter(|_| !is_zip(target)) {
            destinations.push(("the registration frame".to_owned(), target.join(report.file)));
        }
    }
    // Everything is computed before anything is written, so an error leaves no files.
    let thickness = project.stencils.thickness_mm;
    let mut outputs: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    if let Some(path) = &args.three_mf {
        outputs.push((path.clone(), plates_3mf(&plates, thickness).map_err(|e| e.to_string())?));
    }
    if let (Some(path), Some(frame)) = (&args.frame, &frame) {
        outputs.push((path.clone(), frame.stl(thickness).map_err(|e| e.to_string())?));
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
    // Resolve destinations after creating the stencil folder, so a stand-alone
    // frame path and the frame in that folder cannot alias a newly created file.
    if let Err(message) = files::check_destinations(&destinations, &input).and_then(|()| files::write_all(&outputs)) {
        files::remove_dirs(&created);
        return Err(message);
    }

    let written: Vec<&Path> = outputs.iter().map(|(p, _)| p.as_path()).collect();
    let report = Report {
        project: &project,
        map: &map,
        skip: &skip,
        plates: &plates,
        reports: &reports,
        frame: frame_report,
        written: &written,
    };
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
    frame: Option<FrameReport>,
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
                let islands = report.islands.len();
                let largest = report.islands.iter().max_by(|a, b| a.area_mm2.total_cmp(&b.area_mm2));
                let largest = largest.map_or(String::new(), |s| {
                    format!("; the largest, {:.1} x {:.1} mm, is at {}", s.width_mm, s.height_mm, place(s))
                });
                let falls = if islands == 1 {
                    "1 island falls out unless something holds it".to_owned()
                } else {
                    format!("{islands} islands fall out unless something holds them")
                };
                warnings.push(format!("{name}: {falls}{largest}"));
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
            let mut traced_mm2 = report.opening_mm2;
            if let Some(bridges) = &report.flat_bridges {
                traced_mm2 += bridges.covered_mm2;
                let single = &bridges.single_bridge_islands;
                if !single.is_empty() {
                    warnings.push(format!(
                        "{name}: {} only one flat bridge towards the rim, {}",
                        count(single.len(), "island has", "islands have"),
                        places(single)
                    ));
                }
            }
            traced_mm2 = report.traced_mm2.unwrap_or(traced_mm2);
            warnings.extend(Self::shaping_warnings(report, &name));
            if report.paint_mm2 > 0.0 && traced_mm2 < TRACED_ENOUGH * report.paint_mm2 {
                warnings.push(format!(
                    "{name}: only {:.0} % of the paint's area could be traced; detail finer than the {} mm cells is lost",
                    100.0 * traced_mm2 / report.paint_mm2,
                    f64::from(self.map.cell_um) / 1000.0
                ));
            }
        }
        let [bed_w, bed_h] = self.project.stencils.bed_mm;
        if let Some(report) = self.reports.first().filter(|r| !r.fits_bed) {
            warnings.push(format!(
                "the plates, {:.1} x {:.1} mm, do not fit a {bed_w} x {bed_h} mm bed",
                report.width_mm, report.height_mm
            ));
        }
        if let Some(frame) = self.frame.filter(|frame| !frame.fits_bed) {
            warnings.push(format!(
                "the registration frame, {:.1} x {:.1} mm, does not fit a {bed_w} x {bed_h} mm bed",
                frame.width_mm, frame.height_mm
            ));
        }
        warnings
    }

    fn shaping_warnings(report: &PlateReport, name: &str) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Some(raised) = &report.raised_connectors {
            let single = &raised.single_support_islands;
            if !single.is_empty() {
                warnings.push(format!(
                    "{name}: {} only one raised support towards the rim, {}",
                    count(single.len(), "island has", "islands have"),
                    places(single),
                ));
            }
            if raised.long_spans > 0 {
                warnings.push(format!(
                    "{name}: {} may be too long to bridge in printing",
                    count(raised.long_spans, "free span", "free spans"),
                ));
            }
        }
        if report.unplaced_supports > 0 {
            let supports = count(
                report.unplaced_supports,
                "saved support no longer fits and was left out",
                "saved supports no longer fit and were left out",
            );
            warnings.push(format!("{name}: {supports}"));
        }
        if let Some(mark) = &report.mark
            && !mark.applied
        {
            warnings.push(format!("{name}: there is not enough solid margin or thickness to deboss {}.", mark.text));
        }
        if report.chamfer_limited {
            warnings.push(format!("{name}: the chamfer is limited to keep the rim around the openings."));
        }
        if report.overlap_limited {
            warnings.push(format!("{name}: paint overlap is limited to keep the stencil intact."));
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
        let mut report = json!({
            "mode": "freeform",
            "size_mm": [um(map.width_mm()), um(map.height_mm())],
            "cells": [map.width, map.height],
            "cell_mm": f64::from(map.cell_um) / 1000.0,
            "thickness_mm": self.project.stencils.thickness_mm,
            "palette": palette,
            "plates": reports_json(self.plates, self.reports),
            "written": self.written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "warnings": warnings,
        });
        if let Some(frame) = self.frame {
            report["frame"] = json!(frame);
        }
        report
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
        } else {
            println!("plates   none (every paint is skipped)");
        }
        for r in self.reports {
            let supports = r.flat_bridges.as_ref().map_or_else(String::new, |flat| {
                format!(", {} covering {:.1} mm²", count(flat.count, "flat bridge", "flat bridges"), flat.covered_mm2)
            });
            let supports = r.raised_connectors.as_ref().map_or(supports, |raised| {
                format!(", {}", count(raised.count, "trial raised connector", "trial raised connectors"))
            });
            let matched =
                r.ral.map_or_else(String::new, |ral| format!(", RAL {} (approx. {})", ral.code, hex(ral.colour)));
            println!(
                "  {:>2}  {}  {}, {}, {}, {}{supports}{matched}",
                r.paint + 1,
                hex(r.colour),
                count(r.openings, "opening", "openings"),
                count(r.islands.len(), "island", "islands"),
                count(r.thin_webs.len(), "thin web", "thin webs"),
                count(r.narrow_openings.len(), "narrow opening", "narrow openings"),
            );
        }
        if let Some(frame) = self.frame {
            println!(
                "frame    {:.1} x {:.1} mm, {} mm clearance at each edge  {}",
                frame.width_mm, frame.height_mm, frame.clearance_mm, frame.file
            );
        }
        for path in self.written {
            println!("wrote    {}", path.display());
        }
    }
}
