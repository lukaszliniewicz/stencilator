//! Print geometry: fitting a grid of square tiles into a physical canvas at a given DPI.
//!
//! Everything is quantised to whole device pixels at the chosen DPI, so the raster chart,
//! the vector PDF and the stencils share exactly the same geometry.

use crate::Error;
use crate::text::thousands;

/// Millimetres per inch.
pub const MM_PER_INCH: f64 = 25.4;

/// Most tiles in one chart: 500 x 500, a 1 m canvas of 2 mm tiles.
pub const MAX_TILES: u64 = 250_000;

/// A grid of square tiles, separated and surrounded by lines, measured in device pixels.
///
/// Along each axis the pattern is `line, tile, line, tile, ..., tile, line`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridLayout {
    /// Tiles across.
    pub tiles_x: u32,
    /// Tiles down.
    pub tiles_y: u32,
    /// Side of a tile in device pixels.
    pub tile_px: u32,
    /// Width of a grid line in device pixels; 0 when tiles touch.
    pub line_px: u32,
    /// Device pixels per inch.
    pub dpi: u32,
}

impl GridLayout {
    /// Distance from the start of one tile to the start of the next.
    pub fn pitch_px(&self) -> u64 {
        u64::from(self.tile_px) + u64::from(self.line_px)
    }

    /// Width of the whole grid, outer lines included.
    pub fn width_px(&self) -> u64 {
        u64::from(self.tiles_x) * self.pitch_px() + u64::from(self.line_px)
    }

    /// Height of the whole grid, outer lines included.
    pub fn height_px(&self) -> u64 {
        u64::from(self.tiles_y) * self.pitch_px() + u64::from(self.line_px)
    }

    /// Number of tiles.
    pub fn tile_count(&self) -> u64 {
        u64::from(self.tiles_x) * u64::from(self.tiles_y)
    }

    /// A length in device pixels as millimetres.
    #[allow(clippy::cast_precision_loss, reason = "pixel counts are far below 2^52")]
    pub fn px_to_mm(&self, px: u64) -> f64 {
        px as f64 / f64::from(self.dpi) * MM_PER_INCH
    }

    /// Width of the whole grid in millimetres.
    pub fn width_mm(&self) -> f64 {
        self.px_to_mm(self.width_px())
    }

    /// Height of the whole grid in millimetres.
    pub fn height_mm(&self) -> f64 {
        self.px_to_mm(self.height_px())
    }

    /// Side of a tile in millimetres, as printed.
    pub fn tile_mm(&self) -> f64 {
        self.px_to_mm(u64::from(self.tile_px))
    }

    /// Width of a grid line in millimetres, as printed.
    pub fn line_mm(&self) -> f64 {
        self.px_to_mm(u64::from(self.line_px))
    }

    /// The same tiles packed edge to edge.
    #[must_use]
    pub fn without_lines(&self) -> GridLayout {
        GridLayout { line_px: 0, ..*self }
    }
}

/// A length in millimetres as the nearest whole number of device pixels. Halves round to
/// even.
#[allow(clippy::cast_possible_truncation, reason = "saturates; callers reject absurd sizes")]
pub fn mm_to_px(mm: f64, dpi: u32) -> i64 {
    (mm / MM_PER_INCH * f64::from(dpi)).round_ties_even() as i64
}

/// Fits the largest tile grid with the image's aspect ratio inside the canvas.
///
/// The grid never exceeds the canvas. The limiting axis is filled completely and the other
/// axis gets the tile count closest to the image's aspect ratio. A non-zero line width
/// always gives at least a one-pixel line.
///
/// # Errors
///
/// [`Error::Layout`] when the image is empty, a size is not a number, the tile is under a
/// pixel, the canvas cannot hold one tile, or the grid would exceed [`MAX_TILES`].
#[allow(
    clippy::similar_names,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "pixel and tile counts are bounded by the checks above each conversion"
)]
pub fn fit_layout(
    image_size: (u32, u32),
    canvas_mm: (f64, f64),
    tile_mm: f64,
    line_mm: f64,
    dpi: u32,
) -> Result<GridLayout, Error> {
    let (image_w, image_h) = image_size;
    if image_w < 1 || image_h < 1 {
        return Err(Error::Layout("The image is empty.".to_owned()));
    }
    if dpi < 1 {
        return Err(Error::Layout("DPI must be positive.".to_owned()));
    }
    if ![canvas_mm.0, canvas_mm.1, tile_mm, line_mm].iter().all(|v| v.is_finite()) {
        return Err(Error::Layout("Sizes must be finite numbers.".to_owned()));
    }

    let (tile_px, line_px) = tile_and_line_px(tile_mm, line_mm, dpi)?;

    // Floor, so that the finished grid never exceeds the physical canvas.
    let canvas_w_px = (canvas_mm.0 / MM_PER_INCH * f64::from(dpi) + 1e-9).floor() as i64;
    let canvas_h_px = (canvas_mm.1 / MM_PER_INCH * f64::from(dpi) + 1e-9).floor() as i64;
    let pitch = tile_px.saturating_add(line_px);
    let max_x = canvas_w_px.saturating_sub(line_px).div_euclid(pitch);
    let max_y = canvas_h_px.saturating_sub(line_px).div_euclid(pitch);
    if max_x < 1 || max_y < 1 {
        return Err(Error::Layout(
            "The canvas is too small for a single tile. Enlarge it or use smaller tiles.".to_owned(),
        ));
    }

    let scale = (max_x as f64 / f64::from(image_w)).min(max_y as f64 / f64::from(image_h));
    let tiles_x = max_x.min(((f64::from(image_w) * scale).round_ties_even() as i64).max(1));
    let tiles_y = max_y.min(((f64::from(image_h) * scale).round_ties_even() as i64).max(1));
    let tiles = tiles_x.saturating_mul(tiles_y);
    if u64::try_from(tiles).unwrap_or(u64::MAX) > MAX_TILES {
        return Err(Error::Layout(format!(
            "That makes {} tiles; the limit is {}. Use larger tiles or a smaller canvas.",
            thousands(tiles.unsigned_abs()),
            thousands(MAX_TILES)
        )));
    }
    Ok(GridLayout { tiles_x: px(tiles_x)?, tiles_y: px(tiles_y)?, tile_px: px(tile_px)?, line_px: px(line_px)?, dpi })
}

/// One tile per cell of a `grid` of columns and rows, such as a picture's own pixels; the
/// canvas follows from the tile and line sizes.
///
/// # Errors
///
/// [`Error::Layout`] when the grid is empty, a size is not a number, the tile is under a
/// pixel, or the grid would exceed [`MAX_TILES`].
pub fn pixel_layout(grid: (u32, u32), tile_mm: f64, line_mm: f64, dpi: u32) -> Result<GridLayout, Error> {
    let (tiles_x, tiles_y) = grid;
    if tiles_x < 1 || tiles_y < 1 {
        return Err(Error::Layout("The image is empty.".to_owned()));
    }
    if dpi < 1 {
        return Err(Error::Layout("DPI must be positive.".to_owned()));
    }
    if !tile_mm.is_finite() || !line_mm.is_finite() {
        return Err(Error::Layout("Sizes must be finite numbers.".to_owned()));
    }
    let (tile_px, line_px) = tile_and_line_px(tile_mm, line_mm, dpi)?;
    let tiles = u64::from(tiles_x) * u64::from(tiles_y);
    if tiles > MAX_TILES {
        return Err(Error::Layout(format!(
            "The picture is {tiles_x} by {tiles_y} pixels, which makes {} tiles; the limit is {}. Fit it to a canvas instead.",
            thousands(tiles),
            thousands(MAX_TILES)
        )));
    }
    Ok(GridLayout { tiles_x, tiles_y, tile_px: px(tile_px)?, line_px: px(line_px)?, dpi })
}

/// The tile and line widths in whole device pixels. A non-zero line is at least one pixel.
fn tile_and_line_px(tile_mm: f64, line_mm: f64, dpi: u32) -> Result<(i64, i64), Error> {
    let tile_px = mm_to_px(tile_mm, dpi);
    if tile_px < 1 {
        return Err(Error::Layout("The tile is smaller than one pixel at this DPI.".to_owned()));
    }
    let line_px = if line_mm > 0.0 { mm_to_px(line_mm, dpi).max(1) } else { 0 };
    Ok((tile_px, line_px))
}

fn px(value: i64) -> Result<u32, Error> {
    u32::try_from(value).map_err(|_| Error::Layout("The tile or line is too large at this DPI.".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(image: (u32, u32), canvas: (f64, f64), tile: f64, line: f64, dpi: u32) -> GridLayout {
        fit_layout(image, canvas, tile, line, dpi).unwrap()
    }

    #[test]
    fn grid_never_exceeds_the_canvas() {
        for (w, h) in [(1, 1), (64, 48), (1000, 7), (7, 1000), (333, 333)] {
            for dpi in [72, 150, 300, 600] {
                let g = fit((w, h), (210.0, 297.0), 3.3, 0.5, dpi);
                assert!(g.width_mm() <= 210.0 + 1e-9 && g.height_mm() <= 297.0 + 1e-9, "{w}x{h} at {dpi}: {g:?}");
            }
        }
    }

    #[test]
    fn non_finite_sizes_are_refused() {
        assert!(fit_layout((10, 10), (f64::NAN, 100.0), 5.0, 0.8, 300).is_err());
        assert!(fit_layout((10, 10), (100.0, 100.0), f64::INFINITY, 0.8, 300).is_err());
    }

    #[test]
    fn pixel_layouts_take_the_grid_as_it_is() {
        let g = pixel_layout((22, 22), 10.0, 0.8, 300).unwrap();
        assert_eq!((g.tiles_x, g.tiles_y, g.tile_px, g.line_px), (22, 22, 118, 9));
        assert!((g.width_mm() - 237.32).abs() < 0.01, "{}", g.width_mm());
        assert_eq!(pixel_layout((3, 2), 5.0, 0.0, 300).unwrap().line_px, 0);
    }

    #[test]
    fn pixel_layouts_refuse_what_fit_layouts_refuse() {
        assert!(pixel_layout((0, 5), 5.0, 0.8, 300).is_err());
        assert!(pixel_layout((5, 5), 0.01, 0.8, 300).is_err());
        assert!(pixel_layout((5, 5), f64::NAN, 0.8, 300).is_err());
        let error = pixel_layout((800, 600), 5.0, 0.8, 300).unwrap_err().to_string();
        assert!(error.contains("800 by 600") && error.contains("480,000"), "{error}");
    }

    #[test]
    fn huge_negative_canvases_are_refused_not_overflowed() {
        assert!(fit_layout((10, 10), (-1e18, 100.0), 5.0, 0.8, 300).is_err());
    }
}
