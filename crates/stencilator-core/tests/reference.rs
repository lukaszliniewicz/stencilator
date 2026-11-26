//! The engine against reference results in `fixtures/reference`, computed
//! independently with NumPy and Pillow.

#![allow(clippy::disallowed_methods, clippy::unwrap_used, reason = "tests read their fixtures from disk")]

use std::fmt::Write as _;
use std::path::PathBuf;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use stencilator_core::chart::{ChartSettings, build_chart, render, sample_tiles};
use stencilator_core::colour::{lab_to_srgb, srgb_to_lab};
use stencilator_core::layout::fit_layout;
use stencilator_core::quantise::{ColourMode, quantise};

#[derive(Deserialize)]
struct File<T> {
    cases: T,
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/reference").join(name)
}

fn fixture<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = fixture_path(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str::<File<T>>(&text).unwrap().cases
}

#[derive(Deserialize)]
struct LayoutCase {
    name: String,
    image_size: (u32, u32),
    canvas_mm: (f64, f64),
    tile_mm: f64,
    line_mm: f64,
    dpi: u32,
    error: Option<String>,
    layout: Option<ExpectedLayout>,
}

#[derive(Deserialize)]
struct ExpectedLayout {
    tiles_x: u32,
    tiles_y: u32,
    tile_px: u32,
    line_px: u32,
    pitch_px: u64,
    width_px: u64,
    height_px: u64,
    width_mm: f64,
    height_mm: f64,
    tile_mm: f64,
    line_mm: f64,
}

#[test]
fn layouts_match() {
    let cases: Vec<LayoutCase> = fixture("layout.json");
    assert_eq!(cases.len(), 13);
    for case in cases {
        let got = fit_layout(case.image_size, case.canvas_mm, case.tile_mm, case.line_mm, case.dpi);
        match (got, case.error, case.layout) {
            (Err(error), Some(message), None) => assert_eq!(error.message(), message, "{}", case.name),
            (Ok(g), None, Some(e)) => {
                let name = &case.name;
                assert_eq!(
                    (g.tiles_x, g.tiles_y, g.tile_px, g.line_px),
                    (e.tiles_x, e.tiles_y, e.tile_px, e.line_px),
                    "{name}"
                );
                assert_eq!(
                    (g.pitch_px(), g.width_px(), g.height_px()),
                    (e.pitch_px, e.width_px, e.height_px),
                    "{name}"
                );
                // Same formula, same order of operations: the same bits.
                assert_eq!(
                    [g.width_mm(), g.height_mm(), g.tile_mm(), g.line_mm()],
                    [e.width_mm, e.height_mm, e.tile_mm, e.line_mm],
                    "{name}"
                );
            }
            (got, error, layout) => panic!("{}: got {got:?}, expected {error:?} / {:?}", case.name, layout.is_some()),
        }
    }
}

#[derive(Deserialize)]
struct ColourCases {
    rgb: Vec<[u8; 3]>,
    lab: Vec<[f64; 3]>,
    rgb_from_lab: Vec<[u8; 3]>,
}

#[test]
fn colour_conversion_matches() {
    let cases: ColourCases = fixture("colour.json");
    assert_eq!(cases.rgb.len(), 127);
    for ((rgb, lab), back) in cases.rgb.iter().zip(&cases.lab).zip(&cases.rgb_from_lab) {
        let got = srgb_to_lab(*rgb);
        for d in 0..3 {
            assert!((got[d] - lab[d]).abs() < 1e-12, "{rgb:?}: {got:?} against {lab:?}");
        }
        assert_eq!(lab_to_srgb(*lab), *back, "{lab:?}");
    }
}

#[derive(Deserialize)]
struct PaletteCase {
    name: String,
    mode: String,
    n_colours: usize,
    rgb: Vec<[u8; 3]>,
    palette: Vec<[u8; 3]>,
    labels: Vec<u32>,
}

#[test]
fn exact_palettes_match() {
    let cases: Vec<PaletteCase> = fixture("palettes.json");
    assert_eq!(cases.len(), 7);
    for case in cases {
        let mode: ColourMode = case.mode.parse().unwrap();
        let got = quantise(&case.rgb, mode, case.n_colours, 0).unwrap();
        assert_eq!(got.colours, case.palette, "{}", case.name);
        assert_eq!(got.labels, case.labels, "{}", case.name);
    }
}

#[derive(Deserialize)]
struct ChartCase {
    name: String,
    image: String,
    settings: Settings,
    tiles: (u32, u32),
    palette: Vec<[u8; 3]>,
    labels: Vec<Vec<u32>>,
    render: Render,
    stencils: Vec<StencilStats>,
    pdf: PdfParts,
    pdf_tiles_only: PdfParts,
}

#[derive(Deserialize)]
struct StencilStats {
    colour: [u8; 3],
    triangles: usize,
    volume_mm3: f64,
    min_mm: [f64; 3],
    max_mm: [f64; 3],
}

#[derive(Deserialize, Debug, PartialEq)]
struct PdfParts {
    media_box: String,
    content: String,
}

#[derive(Deserialize)]
struct Settings {
    canvas_mm: (f64, f64),
    tile_mm: f64,
    line_mm: f64,
    dpi: u32,
    mode: String,
    n_colours: usize,
    background: [u8; 3],
}

#[derive(Deserialize)]
struct Render {
    grid_colour: [u8; 3],
    size: (u32, u32),
    sha256: String,
}

fn load_chart(case: &ChartCase) -> stencilator_core::chart::TileChart {
    let bytes = std::fs::read(fixture_path(&case.image)).unwrap();
    let image = stencilator_core::source::decode(&bytes).unwrap();
    let s = &case.settings;
    let settings = ChartSettings {
        sizing: stencilator_core::chart::Sizing::Canvas,
        canvas_mm: s.canvas_mm,
        tile_mm: s.tile_mm,
        line_mm: s.line_mm,
        dpi: s.dpi,
        mode: s.mode.parse().unwrap(),
        n_colours: s.n_colours,
        background: s.background,
        seed: 0,
    };
    build_chart(&image, &settings).unwrap()
}

#[test]
fn charts_match_to_the_pixel() {
    let cases: Vec<ChartCase> = fixture("charts.json");
    assert_eq!(cases.len(), 4);
    for case in cases {
        let chart = load_chart(&case);
        let name = &case.name;
        assert_eq!((chart.layout.tiles_x, chart.layout.tiles_y), case.tiles, "{name}");
        assert_eq!(chart.palette, case.palette, "{name}");
        assert_eq!(chart.labels, case.labels.concat(), "{name}");
        let image = render(&chart, case.render.grid_colour, case.settings.line_mm > 0.0).unwrap();
        assert_eq!((image.width, image.height), case.render.size, "{name}");
        let digest = Sha256::digest(&image.rgb).iter().fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        });
        assert_eq!(digest, case.render.sha256, "{name}");
    }
}

#[derive(Deserialize)]
struct Resamples {
    image: String,
    cases: Vec<ResampleCase>,
}

#[derive(Deserialize)]
struct ResampleCase {
    size: (u32, u32),
    background: [u8; 3],
    rgb: Vec<[u8; 3]>,
}

#[test]
fn box_filter_and_compositing_match_pillow() {
    let resamples: Resamples = fixture("resample.json");
    let bytes = std::fs::read(fixture_path(&resamples.image)).unwrap();
    let image = stencilator_core::source::decode(&bytes).unwrap();
    assert_eq!(resamples.cases.len(), 16);
    for case in resamples.cases {
        let got = sample_tiles(&image, case.size.0, case.size.1, case.background);
        assert_eq!(got, case.rgb, "{:?} on {:?}", case.size, case.background);
    }
}

fn pdf_parts(pdf: &[u8]) -> PdfParts {
    let text = String::from_utf8_lossy(pdf);
    let start = text.find("/MediaBox [").unwrap() + "/MediaBox [".len();
    let media_box = text[start..start + text[start..].find(']').unwrap()].to_owned();
    let find = |needle: &[u8]| pdf.windows(needle.len()).position(|w| w == needle).unwrap();
    let (from, to) = (find(b"stream\n") + 7, find(b"\nendstream"));
    let content = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[from..to]).unwrap();
    PdfParts { media_box, content: String::from_utf8(content).unwrap() }
}

#[test]
fn pdf_drawing_commands_match() {
    use stencilator_core::export::chart_pdf;
    let cases: Vec<ChartCase> = fixture("charts.json");
    for case in &cases {
        let chart = load_chart(case);
        let grid = case.render.grid_colour;
        assert_eq!(
            pdf_parts(&chart_pdf(&chart, grid, case.settings.line_mm > 0.0).unwrap()),
            case.pdf,
            "{}",
            case.name
        );
        assert_eq!(pdf_parts(&chart_pdf(&chart, grid, false).unwrap()), case.pdf_tiles_only, "{}", case.name);
    }
}

#[test]
fn stencils_match() {
    use stencilator_core::stencil::stencil_triangles;
    let cases: Vec<ChartCase> = fixture("charts.json");
    let mut checked = 0;
    for case in &cases {
        let chart = load_chart(case);
        for (index, expected) in (0..).zip(&case.stencils) {
            let holes: Vec<bool> = chart.labels.iter().map(|&l| l == index).collect();
            let triangles = stencil_triangles(&holes, &chart.layout, 1.2).unwrap();
            let name = format!("{}, {:?}", case.name, expected.colour);
            assert_eq!(chart.palette[index as usize], expected.colour, "{name}");
            assert_eq!(triangles.len(), expected.triangles, "{name}");
            let volume: f64 = triangles
                .iter()
                .map(|[a, b, c]| {
                    a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                        + a[2] * (b[0] * c[1] - b[1] * c[0])
                })
                .sum::<f64>()
                / 6.0;
            assert!((volume - expected.volume_mm3).abs() < 1e-9, "{name}: {volume} against {}", expected.volume_mm3);
            let corners = triangles.iter().flatten();
            let min = corners.clone().fold([f64::INFINITY; 3], |m, p| [0, 1, 2].map(|d| m[d].min(p[d])));
            let max = corners.fold([f64::NEG_INFINITY; 3], |m, p| [0, 1, 2].map(|d| m[d].max(p[d])));
            assert_eq!((min, max), (expected.min_mm, expected.max_mm), "{name}");
            checked += 1;
        }
    }
    assert_eq!(checked, 15);
}
