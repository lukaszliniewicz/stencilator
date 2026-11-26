//! Spike 4: traces every paint of a drawing, measures each outline against a
//! fine render, draws the outlines over the artwork, and writes one plate per
//! paint.
//!
//! Usage: `phase0-trace <svg> <font> <out-dir> [px-per-mm=10] [blur-passes=1] [tolerance-um=20]`
//! or `phase0-trace <png|jpeg|webp> <width-mm> <out-dir> [...]` for a raster
//! image, which has no vector reference to measure against.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::large_enum_variant,
    clippy::similar_names
)]

use std::path::Path;
use std::process::ExitCode;

use phase0::mesh::{self, Pt, Slab};
use phase0::render::{self, Raster};
use phase0::{digest, inspect, raster, trace};

const REFERENCE_PX_PER_MM: f32 = 40.0;
const MARGIN_MM: f64 = 5.0;
const OUTLINE_COLOURS: [[u8; 3]; 4] = [[0, 210, 90], [230, 0, 200], [0, 170, 255], [250, 220, 0]];

fn save(raster: &Raster, path: &Path) -> Result<(), String> {
    image::RgbImage::from_raw(raster.width, raster.height, raster.on_white())
        .ok_or("bad raster")?
        .save(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn crop(raster: &Raster, x0: u32, y0: u32, w: u32, h: u32) -> Raster {
    let (w, h) = (w.min(raster.width - x0), h.min(raster.height - y0));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y0 + h {
        let start = ((y * raster.width + x0) * 4) as usize;
        rgba.extend_from_slice(&raster.rgba[start..start + (w * 4) as usize]);
    }
    Raster { width: w, height: h, rgba }
}

/// The drawing: vector artwork, or a raster image at a stated width.
enum Source {
    Svg(usvg::Tree),
    Image(Raster, f32, f32),
}

impl Source {
    fn size_mm(&self) -> (f32, f32) {
        match self {
            Source::Svg(tree) => render::size_mm(tree),
            Source::Image(_, w, h) => (*w, *h),
        }
    }

    fn picture(&self, px_per_mm: f32) -> Result<Raster, String> {
        match self {
            Source::Svg(tree) => render::render(tree, px_per_mm),
            Source::Image(image, w, h) => {
                raster::resample(image, (w * px_per_mm).ceil() as u32, (h * px_per_mm).ceil() as u32)
            }
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        return Err("usage: phase0-trace <svg> <font> <out-dir> [px-per-mm] [blur] [tolerance-um]".to_owned());
    }
    let arg = |i: usize, default: f64| -> Result<f64, String> {
        args.get(i).map_or(Ok(default), |s| s.parse().map_err(|e| format!("argument {i}: {e}")))
    };
    let (px_per_mm, blur, tolerance) = (arg(4, 10.0)? as f32, arg(5, 1.0)? as usize, arg(6, 20.0)?);
    let input = std::fs::read(&args[1]).map_err(|e| format!("{}: {e}", args[1]))?;
    let source = if Path::new(&args[1]).extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
        let font = std::fs::read(&args[2]).map_err(|e| format!("{}: {e}", args[2]))?;
        let svg = String::from_utf8(input).map_err(|e| format!("{}: {e}", args[1]))?;
        Source::Svg(render::parse(&svg, &[&font])?)
    } else {
        let image = raster::decode(&input)?;
        let width_mm: f32 = args[2].parse().map_err(|e| format!("width in mm: {e}"))?;
        let height_mm = width_mm * image.height as f32 / image.width as f32;
        Source::Image(image, width_mm, height_mm)
    };
    let out = Path::new(&args[3]);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;

    let (width_mm, height_mm) = source.size_mm();
    let height_um = f64::from(height_mm) * 1000.0;
    let raster = source.picture(px_per_mm)?;
    let palette = trace::palette(&raster, 0.005);
    let map = trace::classify(&raster, &palette);
    let fine = match &source {
        Source::Svg(_) => Some(source.picture(REFERENCE_PX_PER_MM)?),
        Source::Image(..) => None,
    };
    let fine_map = fine.as_ref().map(|f| trace::classify(f, &palette));
    println!("{width_mm:.1} x {height_mm:.1} mm traced at {px_per_mm} px/mm, blur {blur}, tolerance {tolerance} um");

    let mut traced = Vec::new();
    for (i, colour) in palette.iter().enumerate() {
        let label = (i + 1) as u8;
        let shapes = trace::trace(&map, label, 1000.0 / f64::from(px_per_mm), height_um, blur, tolerance);
        let perimeter = inspect::perimeter_um(&shapes);
        let deviation = match (&fine, &fine_map) {
            (Some(fine), Some(fine_map)) => {
                let reference: Vec<bool> = fine_map.labels.iter().map(|&l| l == label).collect();
                let mask = inspect::fill_mask(&shapes, fine.width, fine.height, REFERENCE_PX_PER_MM, height_um as f32);
                format!("{:.1} um", inspect::mean_deviation_um(&reference, &mask, REFERENCE_PX_PER_MM, perimeter))
            }
            _ => "n/a (raster)".to_owned(),
        };
        let contours: usize = shapes.iter().map(Vec::len).sum();
        let points: usize = shapes.iter().flatten().map(Vec::len).sum();
        let bytes: Vec<u8> =
            shapes.iter().flatten().flatten().flat_map(|p| [p.x.to_le_bytes(), p.y.to_le_bytes()].concat()).collect();
        println!(
            "paint {label} #{:02x}{:02x}{:02x}: {} shapes, {contours} contours, {points} points, \
             outline {:.0} mm, mean deviation {deviation}, digest {}",
            colour[0],
            colour[1],
            colour[2],
            shapes.len(),
            perimeter / 1000.0,
            digest::hex(&bytes)
        );

        // A plate for this paint: a margin around the drawing, minus the paint.
        let m = (MARGIN_MM * 1000.0) as i32;
        let (w, h) = ((f64::from(width_mm) * 1000.0) as i32, height_um as i32);
        let plate = vec![vec![vec![Pt::new(-m, -m), Pt::new(w + m, -m), Pt::new(w + m, h + m), Pt::new(-m, h + m)]]];
        let solid = mesh::boolean(&plate, &shapes, mesh::Rule::Difference);
        let plate_mesh = mesh::mesh_slabs(&[Slab { z0: 0, z1: 1200, shapes: solid }])?;
        let report = mesh::check(&plate_mesh);
        let stl_path = out.join(format!("plate-{label}.stl"));
        std::fs::write(&stl_path, mesh::to_stl(&plate_mesh, "phase0 traced plate")).map_err(|e| e.to_string())?;
        println!(
            "  plate {}: {} triangles, open edges {}, repeated edges {}, pieces {}",
            stl_path.display(),
            report.triangles,
            report.open_edges,
            report.repeated_edges,
            report.components
        );
        traced.push(shapes);
    }

    let coloured: Vec<(&mesh::Shapes, [u8; 3])> =
        traced.iter().zip(OUTLINE_COLOURS.iter().cycle()).map(|(s, c)| (s, *c)).collect();
    let mut overview = source.picture(12.0)?;
    inspect::draw_outlines(&mut overview, &coloured, 12.0, height_um as f32, 1.0)?;
    save(&overview, &out.join("overview.png"))?;
    let zoom_ppm = 60.0;
    let mut zoom = source.picture(zoom_ppm)?;
    inspect::draw_outlines(&mut zoom, &coloured, zoom_ppm, height_um as f32, 1.5)?;
    let (x0, y0) = ((width_mm * 0.18 * zoom_ppm) as u32, (height_mm * 0.4 * zoom_ppm) as u32);
    save(
        &crop(&zoom, x0, y0, (width_mm * 0.64 * zoom_ppm) as u32, (height_mm * 0.38 * zoom_ppm) as u32),
        &out.join("zoom.png"),
    )?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("phase0-trace: {error}");
            ExitCode::FAILURE
        }
    }
}
