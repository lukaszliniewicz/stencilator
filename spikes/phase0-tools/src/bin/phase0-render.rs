//! Spike 1, native side: renders an SVG and prints `"<width>x<height> <digest>"`,
//! the same line the browser build prints.
//!
//! Usage: `phase0-render <svg> <font> <px-per-mm> [out.png]`

use std::process::ExitCode;

use phase0::{digest, render};

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        return Err("usage: phase0-render <svg> <font> <px-per-mm> [out.png]".to_owned());
    }
    let svg = std::fs::read_to_string(&args[1]).map_err(|e| format!("{}: {e}", args[1]))?;
    let font = std::fs::read(&args[2]).map_err(|e| format!("{}: {e}", args[2]))?;
    let px_per_mm: f32 = args[3].parse().map_err(|e| format!("px-per-mm: {e}"))?;
    let tree = render::parse(&svg, &[&font])?;
    let raster = render::render(&tree, px_per_mm)?;
    println!("{}x{} {}", raster.width, raster.height, digest::hex(&raster.rgba));
    if let Some(out) = args.get(4) {
        image::RgbImage::from_raw(raster.width, raster.height, raster.on_white())
            .ok_or("bad raster")?
            .save(out)
            .map_err(|e| format!("{out}: {e}"))?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("phase0-render: {error}");
            ExitCode::FAILURE
        }
    }
}
