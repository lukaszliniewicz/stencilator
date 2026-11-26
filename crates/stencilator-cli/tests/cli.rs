//! The `stencilator` command, run as a user would run it.

#![allow(clippy::unwrap_used, reason = "a failed setup should stop the test")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn reference_image() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/reference/chart-0.png")
}

/// An empty folder holding a copy of the reference image as `in.png`.
fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stencilator-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::copy(reference_image(), dir.join("in.png")).unwrap();
    dir
}

fn stencilator(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_stencilator"))
        .current_dir(dir)
        .args(["build", "in.png"])
        .args(args)
        .output()
        .unwrap();
    (output.status.success(), String::from_utf8_lossy(&output.stderr).into_owned())
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
            && plates
                .iter()
                .all(|n| n.starts_with("stencil_color_") && Path::new(n).extension().is_some_and(|e| e == "stl"))
    );
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
    assert!(stderr.contains("--chart and the input image would both be"), "{stderr}");
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
