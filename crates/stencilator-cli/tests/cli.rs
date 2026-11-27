//! The `stencilator` command, run as a user would run it.

#![allow(clippy::unwrap_used, reason = "a failed setup should stop the test")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn reference_image() -> PathBuf {
    fixture("reference/chart-0.png")
}

/// An empty folder holding a copy of the reference image as `in.png`.
fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stencilator-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::copy(reference_image(), dir.join("in.png")).unwrap();
    dir
}

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(path)
}

/// Runs the command in `dir`; whether it succeeded, the report and the messages.
fn run(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_stencilator")).current_dir(dir).args(args).output().unwrap();
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    (output.status.success(), text(&output.stdout), text(&output.stderr))
}

/// `stencilator tiles in.png` with these options.
fn stencilator(dir: &Path, args: &[&str]) -> (bool, String) {
    let (ok, _, stderr) = run(dir, &[&["tiles", "in.png"][..], args].concat());
    (ok, stderr)
}

/// The JSON report of a successful run.
fn report(dir: &Path, args: &[&str]) -> serde_json::Value {
    let (ok, stdout, stderr) = run(dir, &[args, &["--json"][..]].concat());
    assert!(ok, "{args:?}: {stderr}");
    serde_json::from_str(&stdout).unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn writes_the_chart_and_one_stencil_per_colour() {
    let dir = workspace("ok");
    let (ok, stderr) = stencilator(&dir, &["--canvas", "47x40", "--chart", "chart.pdf", "--stencils", "plates/a"]);
    assert!(ok, "{stderr}");
    assert!(fs::read(dir.join("chart.pdf")).unwrap().starts_with(b"%PDF"));
    let plates = names(&dir.join("plates/a"));
    assert!(
        !plates.is_empty()
            && plates.iter().all(|n| n.starts_with("stencil-") && Path::new(n).extension().is_some_and(|e| e == "stl"))
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stencils_can_go_into_one_zip() {
    let dir = workspace("zip");
    let (ok, stderr) = stencilator(&dir, &["--canvas", "47x40", "--stencils", "plates.zip"]);
    assert!(ok, "{stderr}");
    let archive = fs::read(dir.join("plates.zip")).unwrap();
    assert!(archive.starts_with(b"PK\x03\x04"));
    assert_eq!(names(&dir), ["in.png", "plates.zip"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn clashing_outputs_change_nothing() {
    let dir = workspace("clash");
    fs::write(dir.join("out.png"), b"keep me").unwrap();
    let (ok, stderr) = stencilator(&dir, &["--chart", "out.png", "--tiles", "./out.png"]);
    assert!(!ok);
    assert!(stderr.contains("--tiles and --chart would both be"), "{stderr}");
    let (ok, stderr) = stencilator(&dir, &["--chart", "in.png"]);
    assert!(!ok);
    assert!(stderr.contains("--chart and the input file would both be"), "{stderr}");
    assert_eq!(fs::read(dir.join("out.png")).unwrap(), b"keep me");
    assert_eq!(fs::read(dir.join("in.png")).unwrap(), fs::read(reference_image()).unwrap());
    assert_eq!(names(&dir), ["in.png", "out.png"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_failed_write_leaves_no_trace() {
    let dir = workspace("fail");
    // A folder where the tiles should go makes the last move fail.
    fs::create_dir(dir.join("tiles.png")).unwrap();
    let (ok, _) = stencilator(
        &dir,
        &["--canvas", "47x40", "--chart", "chart.png", "--stencils", "new/plates", "--tiles", "tiles.png"],
    );
    assert!(!ok);
    assert_eq!(names(&dir), ["in.png", "tiles.png"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn settings_out_of_range_are_refused_before_reading_the_image() {
    let dir = workspace("range");
    fs::remove_file(dir.join("in.png")).unwrap();
    for (args, message) in [
        (&["--canvas=-1e18x200"][..], "The canvas width must be between"),
        (&["--dpi", "70000"], "The resolution must be between"),
        (&["--stencils", "plates", "--thickness", "1e300"], "The stencil thickness must be between"),
    ] {
        let (ok, stderr) = stencilator(&dir, args);
        assert!(!ok && stderr.contains(message), "{args:?}: {stderr}");
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pixel_art_gets_one_tile_per_pixel_with_merges_and_skips() {
    let dir = workspace("pixels");
    let fox = fixture("pixel-art/fox.png");
    // White is the canvas colour, so it has no stencil unless asked for.
    let fox = fox.to_str().unwrap();
    let summary =
        report(&dir, &["tiles", fox, "--pixels", "--tile", "10", "--merge", "9c3d17=e8772e", "--stencils", "plates"]);
    assert_eq!(summary["tiles"], serde_json::json!([22, 22]));
    let palette: Vec<(String, u64, Option<String>)> = summary["palette"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["colour"].as_str().unwrap().to_owned(),
                p["tiles"].as_u64().unwrap(),
                p["stencil"].as_str().map(String::from),
            )
        })
        .collect();
    assert_eq!(
        palette,
        [
            ("#2b2320".to_owned(), 16, Some("stencil-01-2b2320.stl".to_owned())),
            ("#e8772e".to_owned(), 210, Some("stencil-02-e8772e.stl".to_owned())),
            ("#ffffff".to_owned(), 258, None),
        ]
    );
    assert_eq!(names(&dir.join("plates")), ["stencil-01-2b2320.stl", "stencil-02-e8772e.stl"]);

    let (ok, stderr) = stencilator(&dir, &["--pixels", "--canvas", "100x100"]);
    assert!(!ok && stderr.contains("--canvas"), "{stderr}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_drawing_is_refused_by_tiles_and_cut_by_freeform() {
    let dir = workspace("drawing");
    let badge = fixture("svg/badge.svg");
    let badge = badge.to_str().unwrap();
    let (ok, _, stderr) = run(&dir, &["tiles", badge]);
    assert!(!ok && stderr.contains("use stencilator freeform for SVG drawings"), "{stderr}");
    let (ok, _, stderr) = run(&dir, &["freeform", badge, "--font-size", "20"]);
    assert!(!ok && stderr.contains("--font-size and --text-colour are for typed text"), "{stderr}");

    let summary = report(&dir, &["freeform", badge, "--stencils", "plates", "--3mf", "badge.3mf"]);
    assert_eq!(summary["mode"], "freeform");
    assert_eq!(summary["size_mm"], serde_json::json!([70.0, 70.0]));
    let stencils: Vec<&str> =
        summary["plates"].as_array().unwrap().iter().map(|p| p["stencil"].as_str().unwrap()).collect();
    assert_eq!(stencils, ["plate-01-1d2f5c.stl", "plate-02-ef7d2d.stl"]);
    assert_eq!(names(&dir.join("plates")), stencils);
    assert!(fs::read(dir.join("badge.3mf")).unwrap().starts_with(b"PK\x03\x04"));
    // The canvas colour is a paint, but has no plate.
    assert_eq!(summary["palette"][2]["colour"], "#ffffff");
    assert!(summary["palette"][2]["stencil"].is_null());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn typed_text_is_saved_as_a_project_that_opens_again() {
    let dir = workspace("text");
    let first = report(&dir, &["freeform", "--text", "AO", "--font-size", "20", "--save", "ao.stencilator"]);
    let plates = first["plates"].as_array().unwrap();
    assert_eq!(plates.len(), 1);
    // The middles of the A and the O.
    assert_eq!(plates[0]["islands"].as_array().unwrap().len(), 2);
    assert!(first["warnings"][0].as_str().unwrap().contains("2 islands fall out"));

    // The project holds the text and its settings, and names the built-in font.
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(dir.join("ao.stencilator")).unwrap()).unwrap();
    assert_eq!(saved["source"]["text"], "AO");
    assert_eq!(saved["fonts"][0]["name"], "NotoSans-Bold.ttf");
    let again = report(&dir, &["freeform", "ao.stencilator"]);
    assert_eq!(again["plates"], first["plates"]);

    // Saving over the project it came from is fine; writing anything else over it is not.
    let finer = report(&dir, &["freeform", "ao.stencilator", "--cell", "0.05", "--save", "ao.stencilator"]);
    assert_eq!(finer["cell_mm"], 0.05);
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(dir.join("ao.stencilator")).unwrap()).unwrap();
    assert_eq!(saved["freeform"]["cell_mm"], 0.05);
    let (ok, _, stderr) = run(&dir, &["freeform", "ao.stencilator", "--3mf", "ao.stencilator"]);
    assert!(!ok && stderr.contains("--3mf and the input project would both be"), "{stderr}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn flat_bridges_survive_project_save_and_can_be_turned_off() {
    let dir = workspace("flat-bridges");
    let original = report(&dir, &["freeform", "--text", "O", "--3mf", "original.3mf"]);
    assert_eq!(original["plates"][0]["islands"].as_array().unwrap().len(), 1);
    let bridged = report(
        &dir,
        &["freeform", "--text", "O", "--flat-bridges", "--3mf", "bridged.3mf", "--save", "bridged.stencilator"],
    );
    let plate = &bridged["plates"][0];
    assert_eq!(plate["islands"], serde_json::json!([]));
    assert_eq!(plate["flat_bridges"]["count"], 2);
    assert!(bridged["warnings"].as_array().unwrap().iter().all(|w| !w.as_str().unwrap().contains("could be traced")));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(dir.join("bridged.stencilator")).unwrap()).unwrap();
    assert_eq!(saved["stencilator"], 2);
    assert_eq!(saved["freeform"]["bridge_width_mm"], 1.2);
    let again = report(&dir, &["freeform", "bridged.stencilator", "--3mf", "reopened.3mf"]);
    let print = |name| fs::read(dir.join(name)).unwrap();
    assert_eq!(again["plates"], bridged["plates"]);
    assert_eq!(print("reopened.3mf"), print("bridged.3mf"));
    assert_ne!(print("bridged.3mf"), print("original.3mf"));

    let plain = report(
        &dir,
        &["freeform", "bridged.stencilator", "--flat-bridges=0", "--3mf", "plain.3mf", "--save", "plain.stencilator"],
    );
    let saved: serde_json::Value = serde_json::from_slice(&print("plain.stencilator")).unwrap();
    assert_eq!(saved["stencilator"], 1);
    assert_eq!(plain["plates"], original["plates"]);
    assert_eq!(print("plain.3mf"), print("original.3mf"));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn custom_support_dimensions_survive_switching_off_and_back_on() {
    let dir = workspace("support-dimensions");
    let saved = |name| serde_json::from_slice::<serde_json::Value>(&fs::read(dir.join(name)).unwrap()).unwrap();
    report(
        &dir,
        &[
            "freeform",
            "--text",
            "O",
            "--raised-connectors=1.4",
            "--beam-thickness",
            "0.9",
            "--save",
            "raised.stencilator",
        ],
    );
    report(&dir, &["freeform", "raised.stencilator", "--flat-bridges=1.6", "--save", "flat.stencilator"]);
    assert_eq!(saved("flat.stencilator")["freeform"]["bridge_width_mm"], 1.6);
    assert_eq!(saved("flat.stencilator")["freeform"]["inactive_raised_connectors"]["beam_mm"], 0.9);
    let plain = report(&dir, &["freeform", "flat.stencilator", "--flat-bridges=0", "--save", "plain.stencilator"]);
    assert!(plain["plates"][0].get("flat_bridges").is_none());
    assert!(plain["plates"][0].get("raised_connectors").is_none());
    report(&dir, &["freeform", "plain.stencilator", "--flat-bridges", "--save", "flat-again.stencilator"]);
    assert_eq!(saved("flat-again.stencilator")["freeform"], saved("flat.stencilator")["freeform"]);
    report(&dir, &["freeform", "flat-again.stencilator", "--raised-connectors", "--save", "raised-again.stencilator"]);
    assert_eq!(
        saved("raised-again.stencilator")["freeform"]["raised_connectors"],
        saved("raised.stencilator")["freeform"]["raised_connectors"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn keyed_plates_and_their_frame_export_and_reopen_together() {
    let dir = workspace("registration");
    let badge = fixture("svg/badge.svg");
    let badge = badge.to_str().unwrap();
    let plain = report(&dir, &["freeform", badge, "--flat-bridges", "--3mf", "plain.3mf"]);
    let keyed = report(
        &dir,
        &[
            "freeform",
            badge,
            "--flat-bridges",
            "--registration",
            "--bed",
            "95x96",
            "--3mf",
            "set.3mf",
            "--stencils",
            "plates",
            "--frame",
            "frame.stl",
            "--save",
            "keyed.stencilator",
        ],
    );
    assert_eq!(
        keyed["frame"],
        serde_json::json!({
            "width_mm": 100.6, "height_mm": 103.6, "clearance_mm": 0.3,
            "fits_bed": false, "file": "registration-frame.stl"
        })
    );
    for plate in keyed["plates"].as_array().unwrap() {
        assert_eq!(plate["height_mm"], 93.0);
        assert_eq!(plate["fits_bed"], true);
    }
    assert!(keyed["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("frame, 100.6")));
    let file = |name| fs::read(dir.join(name)).unwrap();
    assert_eq!(file("frame.stl"), file("plates/registration-frame.stl"));
    let saved: serde_json::Value = serde_json::from_slice(&file("keyed.stencilator")).unwrap();
    assert_eq!(saved["stencilator"], 3);
    assert_eq!(saved["freeform"]["registration_clearance_mm"], 0.3);
    let again = report(&dir, &["freeform", "keyed.stencilator", "--3mf", "again.3mf"]);
    assert_eq!(again["frame"], keyed["frame"]);
    assert_eq!(file("again.3mf"), file("set.3mf"));
    let disabled = report(
        &dir,
        &[
            "freeform",
            "keyed.stencilator",
            "--registration=0",
            "--3mf",
            "disabled.3mf",
            "--save",
            "disabled.stencilator",
        ],
    );
    assert!(disabled.get("frame").is_none());
    assert_eq!(disabled["plates"], plain["plates"]);
    assert_eq!(file("disabled.3mf"), file("plain.3mf"));
    let saved: serde_json::Value = serde_json::from_slice(&file("disabled.stencilator")).unwrap();
    assert_eq!(saved["stencilator"], 2);

    let frame = file("frame.stl");
    let absolute = dir.join("new/registration-frame.stl");
    let (ok, _, stderr) = run(
        &dir,
        &[
            "freeform",
            "keyed.stencilator",
            "--stencils",
            "new",
            "--frame",
            absolute.to_str().unwrap(),
            "--save",
            "clash.stencilator",
        ],
    );
    assert!(!ok && stderr.contains("the registration frame and --frame would both be"), "{stderr}");
    assert!(!dir.join("new").exists());
    assert!(!dir.join("clash.stencilator").exists());
    let (ok, _, stderr) = run(&dir, &["freeform", "disabled.stencilator", "--frame", "frame.stl"]);
    assert!(!ok && stderr.contains("Add --registration"), "{stderr}");
    assert_eq!(file("frame.stl"), frame);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_project_saved_without_an_extension_can_be_reopened() {
    let dir = workspace("project-name");
    let first = report(&dir, &["freeform", "--text", "AO", "--save", "design"]);
    let again = report(&dir, &["freeform", "design"]);
    assert_eq!(again["plates"], first["plates"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_project_can_keep_all_paints_without_making_any_plates() {
    let dir = workspace("skipped");
    let first = report(&dir, &["freeform", "--text", "AO", "--skip", "000000", "--save", "project.stencilator"]);
    assert_eq!(first["plates"], serde_json::json!([]));
    let palette = first["palette"].as_array().unwrap();
    assert_eq!(palette.len(), 2);
    assert!(palette.iter().all(|paint| paint["stencil"].is_null()));
    let again = report(&dir, &["freeform", "project.stencilator", "--save", "copy.stencilator"]);
    assert_eq!(again["palette"], first["palette"]);
    for (flag, path) in [("--3mf", "plates.3mf"), ("--stencils", "plates"), ("--stencils", "plates.zip")] {
        let (ok, _, stderr) =
            run(&dir, &["freeform", "project.stencilator", flag, path, "--save", "another.stencilator"]);
        assert!(!ok && stderr.contains("Every paint is skipped"), "{stderr}");
    }
    assert_eq!(names(&dir), ["copy.stencilator", "in.png", "project.stencilator"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn adding_a_font_to_a_drawing_keeps_its_embedded_font() {
    let dir = workspace("drawing-fonts");
    fs::write(
        dir.join("word.svg"),
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20">
        <text x="5" y="15" font-family="Allerta Stencil" font-size="12">AO</text></svg>"#,
    )
    .unwrap();
    let font = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts/AllertaStencil-Regular.ttf");
    let first =
        report(&dir, &["freeform", "word.svg", "--font", font.to_str().unwrap(), "--save", "first.stencilator"]);
    let again =
        report(&dir, &["freeform", "first.stencilator", "--font", "Noto Sans Bold", "--save", "added.stencilator"]);
    let project =
        |name: &str| -> serde_json::Value { serde_json::from_slice(&fs::read(dir.join(name)).unwrap()).unwrap() };
    let original = project("first.stencilator");
    let added = project("added.stencilator");
    assert!(original["fonts"][0]["data"].is_string());
    assert_eq!(added["fonts"][0]["name"], original["fonts"][0]["name"]);
    assert!(added["fonts"][0]["data"] == original["fonts"][0]["data"], "the embedded font changed");
    assert_eq!(added["fonts"][1]["name"], "NotoSans-Bold.ttf");
    assert_eq!(again["plates"], first["plates"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn freeform_settings_out_of_range_are_refused_before_reading_the_file() {
    let dir = workspace("freeform-range");
    for (args, message) in [
        (&["missing.svg", "--cell", "1"][..], "The cell size must be between"),
        (&["missing.svg", "--margin", "0.5"], "The margin must be between"),
        (&["missing.svg", "--flat-bridges=0.2"], "The flat bridge width must be between"),
        (&["missing.svg", "--registration=1"], "The registration clearance must be between"),
        (&["missing.png", "--width", "1e9"], "The width must be between"),
    ] {
        let (ok, _, stderr) = run(&dir, &[&["freeform"][..], args].concat());
        assert!(!ok && stderr.contains(message), "{args:?}: {stderr}");
    }
    fs::remove_dir_all(&dir).unwrap();
}
