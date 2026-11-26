//! Writes the plates of the cases in a JSON file as STL and all of them as one 3MF, so
//! that independent tools (trimesh and lib3mf, for `fixtures/reference/plates.json`) can
//! check them, and prints each plate's report. A stand-in until the command line has a free-form command.
//!
//! Usage: `plates CASES.json OUT_DIR`, run from the repository root. Each case is
//! `{"name", "kind": "svg" | "picture" | "text", "source", "font_mm"?, "width_mm"?,
//! "cell_mm", "colours", "min_patch_mm2"?, "skip": ["#rrggbb", ...]}`; without `width_mm`
//! the artwork has its own size. Plates go to `OUT_DIR/<name>/`.

use std::path::Path;

use serde_json::Value;
use stencilator_core::artwork::{ArtworkSettings, Size, label_drawing, label_picture};
use stencilator_core::colour::Rgb;
use stencilator_core::drawing::{parse_svg, text_drawing};
use stencilator_core::plate::{PlateSettings, build_plates, check_plate, plates_3mf};

const FONT: &str = "fonts/NotoSans-Bold.ttf";

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [cases, out] = args.as_slice() else {
        return Err("usage: plates CASES.json OUT_DIR".to_owned());
    };
    let text = std::fs::read_to_string(cases).map_err(|e| format!("{cases}: {e}"))?;
    let cases: Vec<Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    for case in &cases {
        let field = |name: &str| case.get(name).ok_or_else(|| format!("a case has no {name}"));
        let string = |name: &str| field(name)?.as_str().map(str::to_owned).ok_or_else(|| format!("{name} is not text"));
        let number = |name: &str| field(name)?.as_f64().ok_or_else(|| format!("{name} is not a number"));
        let name = string("name")?;
        let settings = ArtworkSettings {
            size: case.get("width_mm").and_then(Value::as_f64).map_or(Size::Natural, Size::Width),
            cell_mm: number("cell_mm")?,
            n_colours: usize::try_from(field("colours")?.as_u64().unwrap_or(16)).map_err(|e| e.to_string())?,
            min_patch_mm2: case
                .get("min_patch_mm2")
                .and_then(Value::as_f64)
                .unwrap_or(ArtworkSettings::default().min_patch_mm2),
            ..ArtworkSettings::default()
        };
        let source = string("source")?;
        let read = |path: &str| std::fs::read(path).map_err(|e| format!("{path}: {e}"));
        let map = match string("kind")?.as_str() {
            "svg" => label_drawing(&parse_svg(&read(&source)?, &[&read(FONT)?]).map_err(|e| e.to_string())?, &settings),
            "picture" => {
                let image = stencilator_core::source::decode(&read(&source)?).map_err(|e| e.to_string())?;
                label_picture(&image, &settings)
            }
            "text" => {
                let drawing = text_drawing(&source, &[&read(FONT)?], number("font_mm")?, [0, 0, 0]);
                label_drawing(&drawing.map_err(|e| e.to_string())?, &settings)
            }
            other => return Err(format!("unknown kind {other}")),
        }
        .map_err(|e| e.to_string())?;
        let skip: Vec<Rgb> = field("skip")?
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_str).filter_map(hex).collect())
            .unwrap_or_default();
        let plate_settings = PlateSettings::default();
        let dir = Path::new(out).join(&name);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let plates = build_plates(&map, &plate_settings, &skip).map_err(|e| e.to_string())?;
        let model = plates_3mf(&plates, plate_settings.thickness_mm).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("plates.3mf"), model).map_err(|e| e.to_string())?;
        for plate in plates {
            let stl = plate.stl(plate_settings.thickness_mm).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(plate.file_name()), stl).map_err(|e| e.to_string())?;
            let report = check_plate(&plate, &plate_settings).map_err(|e| e.to_string())?;
            println!(
                "{name} {}: {} openings, {:.1} % of the paint traced, {} islands, {} thin webs, {} narrow openings{}",
                plate.file_name(),
                report.openings,
                100.0 * report.opening_mm2 / report.paint_mm2,
                report.islands.len(),
                report.thin_webs.len(),
                report.narrow_openings.len(),
                if report.fits_bed { "" } else { ", too large for the bed" }
            );
        }
    }
    Ok(())
}

fn hex(text: &str) -> Option<Rgb> {
    let digits = text.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}
