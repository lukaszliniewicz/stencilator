//! Looking at traced outlines: drawing them over the artwork and measuring
//! how far they stray from it.

use resvg::tiny_skia::{self, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::mesh::Shapes;
use crate::render::Raster;

/// A path in pixels for shapes in micrometres (y up from the bottom of a
/// drawing `height_um` tall).
fn path(shapes: &Shapes, px_per_mm: f32, height_um: f32) -> Option<tiny_skia::Path> {
    let scale = px_per_mm / 1000.0;
    let mut builder = PathBuilder::new();
    for contour in shapes.iter().flatten() {
        for (i, p) in contour.iter().enumerate() {
            let (x, y) = (p.x as f32 * scale, (height_um - p.y as f32) * scale);
            if i == 0 {
                builder.move_to(x, y);
            } else {
                builder.line_to(x, y);
            }
        }
        builder.close();
    }
    builder.finish()
}

/// Draws outlines over a raster, one colour per set of shapes.
pub fn draw_outlines(
    raster: &mut Raster,
    outlines: &[(&Shapes, [u8; 3])],
    px_per_mm: f32,
    height_um: f32,
    line_px: f32,
) -> Result<(), String> {
    let mut pixmap = Pixmap::from_vec(
        std::mem::take(&mut raster.rgba),
        tiny_skia::IntSize::from_wh(raster.width, raster.height).ok_or("empty raster")?,
    )
    .ok_or("bad raster")?;
    for (shapes, [r, g, b]) in outlines {
        if let Some(path) = path(shapes, px_per_mm, height_um) {
            let mut paint = Paint::default();
            paint.set_color_rgba8(*r, *g, *b, 255);
            paint.anti_alias = true;
            let stroke = Stroke { width: line_px, ..Stroke::default() };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
    raster.rgba = pixmap.take();
    Ok(())
}

/// Fills shapes into a mask (no anti-aliasing: a pixel is in when its centre is).
pub fn fill_mask(shapes: &Shapes, width: u32, height: u32, px_per_mm: f32, height_um: f32) -> Vec<bool> {
    let Some(mut pixmap) = Pixmap::new(width, height) else {
        return Vec::new();
    };
    if let Some(path) = path(shapes, px_per_mm, height_um) {
        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        paint.anti_alias = false;
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::EvenOdd, Transform::identity(), None);
    }
    pixmap.data().as_chunks::<4>().0.iter().map(|p| p[3] > 127).collect()
}

/// Total outline length in micrometres.
#[must_use]
pub fn perimeter_um(shapes: &Shapes) -> f64 {
    shapes
        .iter()
        .flatten()
        .map(|contour| {
            (0..contour.len())
                .map(|i| {
                    let (a, b) = (contour[i], contour[(i + 1) % contour.len()]);
                    (f64::from(b.x - a.x)).hypot(f64::from(b.y - a.y))
                })
                .sum::<f64>()
        })
        .sum()
}

/// Average distance between two outlines: the area where the masks disagree,
/// divided by the outline length.
#[must_use]
pub fn mean_deviation_um(reference: &[bool], traced: &[bool], px_per_mm: f32, perimeter_um: f64) -> f64 {
    let differing = reference.iter().zip(traced).filter(|(a, b)| a != b).count();
    let px_um = 1000.0 / f64::from(px_per_mm);
    differing as f64 * px_um * px_um / perimeter_um.max(1.0)
}
