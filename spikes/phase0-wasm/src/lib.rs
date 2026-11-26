//! Spike 1: the spike library compiled for the browser.
//!
//! Every function returns the same digests as the native tools, so the two
//! builds can be compared byte for byte.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::missing_errors_doc)]

use phase0::{demo, digest, mesh, raster, render, trace};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// Renders an SVG; returns `"<width>x<height> <digest>"`.
#[wasm_bindgen]
pub fn render_digest(svg: &str, font: &[u8], px_per_mm: f32) -> Result<String, JsError> {
    let tree = render::parse(svg, &[font]).map_err(|e| JsError::new(&e))?;
    let raster = render::render(&tree, px_per_mm).map_err(|e| JsError::new(&e))?;
    Ok(format!("{}x{} {}", raster.width, raster.height, digest::hex(&raster.rgba)))
}

/// Renders an SVG onto white; returns RGBA for a canvas.
#[wasm_bindgen]
pub fn render_on_white(svg: &str, font: &[u8], px_per_mm: f32) -> Result<Vec<u8>, JsError> {
    let tree = render::parse(svg, &[font]).map_err(|e| JsError::new(&e))?;
    let raster = render::render(&tree, px_per_mm).map_err(|e| JsError::new(&e))?;
    Ok(raster.on_white().as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect())
}

/// Width of the rendered SVG in pixels, to size a canvas.
#[wasm_bindgen]
pub fn render_width(svg: &str, font: &[u8], px_per_mm: f32) -> Result<u32, JsError> {
    let tree = render::parse(svg, &[font]).map_err(|e| JsError::new(&e))?;
    Ok((render::size_mm(&tree).0 * px_per_mm).ceil() as u32)
}

/// The connector plate of spike 2 as STL bytes.
#[wasm_bindgen]
pub fn connector_plate_stl(rotation_deg: f64) -> Result<Vec<u8>, JsError> {
    let slabs = demo::connector_plate(rotation_deg).map_err(|e| JsError::new(&e))?;
    let mesh = mesh::mesh_slabs(&slabs).map_err(|e| JsError::new(&e))?;
    Ok(mesh::to_stl(&mesh, "phase0 connector plate"))
}

/// Traces one label of a rendered SVG; returns `"<contours> <points> <digest>"`.
#[wasm_bindgen]
pub fn trace_digest(svg: &str, font: &[u8], px_per_mm: f32, label: u8) -> Result<String, JsError> {
    let tree = render::parse(svg, &[font]).map_err(|e| JsError::new(&e))?;
    let raster = render::render(&tree, px_per_mm).map_err(|e| JsError::new(&e))?;
    let palette = trace::palette(&raster, 0.005);
    let map = trace::classify(&raster, &palette);
    let height_um = f64::from(render::size_mm(&tree).1) * 1000.0;
    let shapes = trace::trace(&map, label, 1000.0 / f64::from(px_per_mm), height_um, 1, 20.0);
    let bytes: Vec<u8> =
        shapes.iter().flatten().flatten().flat_map(|p| [p.x.to_le_bytes(), p.y.to_le_bytes()].concat()).collect();
    let contours: usize = shapes.iter().map(Vec::len).sum();
    Ok(format!("{contours} {} {}", bytes.len() / 8, digest::hex(&bytes)))
}

/// Size of an encoded PNG, JPEG or WebP image; returns `"<width>x<height>"`.
#[wasm_bindgen]
pub fn image_size(bytes: &[u8]) -> Result<String, JsError> {
    let (w, h) = raster::decoded_size(bytes).map_err(|e| JsError::new(&e))?;
    Ok(format!("{w}x{h}"))
}
