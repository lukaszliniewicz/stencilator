//! From an image to a tile chart, and from a chart to pixels.

use std::ops::RangeInclusive;

use crate::Error;
use crate::colour::Rgb;
use crate::layout::{GridLayout, fit_layout, pixel_layout};
use crate::paints::merge_plan;
use crate::quantise::{ColourMode, quantise};
use crate::resample::{flatten, resize_box};
use crate::source::{Image, pixel_grid};

/// Largest chart rendered as pixels: about 750 MB as RGB. The vector PDF has no such limit.
pub const MAX_RASTER_PIXELS: u64 = 250_000_000;

// Accepted settings. Outside these ranges a value is almost certainly a slip of the
// keyboard, and inside them every size stays far from the limits of the integers that
// hold it.

/// Accepted canvas sides in millimetres.
pub const CANVAS_MM: RangeInclusive<f64> = 1.0..=10_000.0;
/// Accepted tile sides in millimetres.
pub const TILE_MM: RangeInclusive<f64> = 0.1..=1_000.0;
/// Accepted grid line widths in millimetres.
pub const LINE_MM: RangeInclusive<f64> = 0.0..=20.0;
/// Accepted resolutions in dots per inch.
pub const DPI: RangeInclusive<u32> = 72..=2_400;
/// Accepted palette sizes.
pub const COLOURS: RangeInclusive<usize> = 1..=256;

/// How the number of tiles is decided.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sizing {
    /// As many tiles as fit the canvas, keeping the picture's proportions.
    #[default]
    Canvas,
    /// One tile per pixel of the picture, for pixel art; the canvas follows. Pixel art
    /// scaled up by a whole factor counts at the size it was drawn.
    Pixels,
}

/// Everything that decides which tiles a chart has and what colour they are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartSettings {
    /// How the number of tiles is decided.
    pub sizing: Sizing,
    /// Width and height of the canvas in millimetres, when the grid is fitted to it.
    pub canvas_mm: (f64, f64),
    /// Side of a tile in millimetres.
    pub tile_mm: f64,
    /// Width of a grid line in millimetres; 0 for none.
    pub line_mm: f64,
    /// Device pixels per inch, for the chart and the geometry.
    pub dpi: u32,
    /// How colours are reduced.
    pub mode: ColourMode,
    /// Most colours in the palette.
    pub n_colours: usize,
    /// Colour that transparent parts of the image take.
    pub background: Rgb,
    /// Seed for the colour clustering.
    pub seed: u64,
}

impl ChartSettings {
    /// Checks every setting against its accepted range.
    ///
    /// # Errors
    ///
    /// [`Error::Settings`] naming the first setting out of range, and the range.
    pub fn check(&self) -> Result<(), Error> {
        let mm = |name: &str, value: f64, range: &RangeInclusive<f64>| {
            if range.contains(&value) {
                Ok(())
            } else {
                Err(Error::Settings(format!("{name} must be between {} and {} mm.", range.start(), range.end())))
            }
        };
        if self.sizing == Sizing::Canvas {
            mm("The canvas width", self.canvas_mm.0, &CANVAS_MM)?;
            mm("The canvas height", self.canvas_mm.1, &CANVAS_MM)?;
        }
        mm("The tile size", self.tile_mm, &TILE_MM)?;
        mm("The line width", self.line_mm, &LINE_MM)?;
        if !DPI.contains(&self.dpi) {
            return Err(Error::Settings(format!(
                "The resolution must be between {} and {} DPI.",
                DPI.start(),
                DPI.end()
            )));
        }
        if !COLOURS.contains(&self.n_colours) {
            return Err(Error::Settings(format!(
                "The number of colours must be between {} and {}.",
                COLOURS.start(),
                COLOURS.end()
            )));
        }
        Ok(())
    }
}

impl Default for ChartSettings {
    fn default() -> Self {
        ChartSettings {
            sizing: Sizing::Canvas,
            canvas_mm: (200.0, 200.0),
            tile_mm: 5.0,
            line_mm: 0.8,
            dpi: 300,
            mode: ColourMode::Colour,
            n_colours: 16,
            background: [255, 255, 255],
            seed: 0,
        }
    }
}

/// A grid layout plus one palette index per tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileChart {
    /// The grid.
    pub layout: GridLayout,
    /// The colours, darkest first.
    pub palette: Vec<Rgb>,
    /// Palette index of every tile, row by row from the top of the image.
    pub labels: Vec<u32>,
}

impl TileChart {
    /// Number of tiles of each palette colour.
    pub fn tile_counts(&self) -> Vec<u64> {
        let mut counts = vec![0; self.palette.len()];
        for &l in &self.labels {
            counts[l as usize] += 1;
        }
        counts
    }

    /// Palette index of the tile in column `x`, row `y` (row 0 at the top).
    pub fn label(&self, x: u32, y: u32) -> u32 {
        self.labels[y as usize * self.layout.tiles_x as usize + x as usize]
    }
}

/// Lays out the grid, samples one colour per tile and reduces the colours to a palette.
///
/// Each tile takes the box-filtered colour of the part of the image it covers, which
/// avoids the halo colours a sharpening filter would add to the palette.
///
/// # Errors
///
/// [`Error::Settings`] when a setting is out of range (see [`ChartSettings::check`]),
/// [`Error::Layout`] when the settings cannot produce a grid.
pub fn build_chart(image: &Image, settings: &ChartSettings) -> Result<TileChart, Error> {
    settings.check()?;
    let layout = match settings.sizing {
        Sizing::Canvas => fit_layout(
            (image.width, image.height),
            settings.canvas_mm,
            settings.tile_mm,
            settings.line_mm,
            settings.dpi,
        )?,
        Sizing::Pixels => pixel_layout(pixel_grid(image), settings.tile_mm, settings.line_mm, settings.dpi)?,
    };
    let rgb = sample_tiles(image, layout.tiles_x, layout.tiles_y, settings.background);
    let palette = quantise(&rgb, settings.mode, settings.n_colours, settings.seed)?;
    Ok(TileChart { layout, palette: palette.colours, labels: palette.labels })
}

/// The chart with paints merged, as [`merge_plan`] describes.
#[must_use]
pub fn merge_paints(chart: &TileChart, merges: &[(Rgb, Rgb)]) -> TileChart {
    let (palette, moved) = merge_plan(&chart.palette, merges);
    let labels = chart.labels.iter().map(|&l| u32::try_from(moved[l as usize]).unwrap_or(u32::MAX)).collect();
    TileChart { layout: chart.layout, palette, labels }
}

/// One colour per tile: the image shrunk with Pillow's box filter, then laid on the
/// background colour. Rows from the top.
pub fn sample_tiles(image: &Image, tiles_x: u32, tiles_y: u32, background: Rgb) -> Vec<Rgb> {
    flatten(&resize_box(image, tiles_x, tiles_y), background)
}

/// RGB pixels, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Three bytes per pixel.
    pub rgb: Vec<u8>,
}

/// For each pixel along one axis, the tile it falls in, or `None` on a grid line.
fn axis_tiles(n_tiles: u32, layout: &GridLayout, size_px: u64) -> Vec<Option<u32>> {
    let (line, pitch, tile) = (u64::from(layout.line_px), layout.pitch_px(), u64::from(layout.tile_px));
    (0..size_px)
        .map(|p| {
            let position = p.checked_sub(line)?;
            let index = u32::try_from(position / pitch).ok()?;
            (position % pitch < tile && index < n_tiles).then_some(index)
        })
        .collect()
}

/// Rasterises the chart at its DPI, with grid lines or with the tiles packed edge to edge.
///
/// # Errors
///
/// [`Error::Export`] when the image would exceed [`MAX_RASTER_PIXELS`].
pub fn render(chart: &TileChart, grid_colour: Rgb, with_lines: bool) -> Result<RgbImage, Error> {
    let layout = if with_lines { chart.layout } else { chart.layout.without_lines() };
    let (width, height) = (layout.width_px(), layout.height_px());
    if width.saturating_mul(height) > MAX_RASTER_PIXELS {
        return Err(Error::Export(format!(
            "{width} × {height} px is too large for an image file (the limit is {} megapixels). \
             Save as PDF, or lower the DPI.",
            MAX_RASTER_PIXELS / 1_000_000
        )));
    }
    let rows = axis_tiles(layout.tiles_y, &layout, height);
    let cols = axis_tiles(layout.tiles_x, &layout, width);
    let mut rgb = Vec::with_capacity(usize::try_from(width * height * 3).unwrap_or(0));
    let line_row: Vec<u8> = (0..width).flat_map(|_| grid_colour).collect();
    let mut tile_rows: Vec<Option<Vec<u8>>> = vec![None; layout.tiles_y as usize];
    for row in rows {
        match row {
            None => rgb.extend_from_slice(&line_row),
            Some(ty) => {
                let pixels = tile_rows[ty as usize].get_or_insert_with(|| {
                    cols.iter()
                        .flat_map(|col| col.map_or(grid_colour, |tx| chart.palette[chart.label(tx, ty) as usize]))
                        .collect()
                });
                rgb.extend_from_slice(pixels);
            }
        }
    }
    let size = |v: u64| u32::try_from(v).map_err(|_| Error::Export("The chart is too large.".to_owned()));
    Ok(RgbImage { width: size(width)?, height: size(height)?, rgb })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart() -> TileChart {
        let layout = GridLayout { tiles_x: 2, tiles_y: 1, tile_px: 2, line_px: 1, dpi: 300 };
        TileChart { layout, palette: vec![[10, 10, 10], [200, 200, 200]], labels: vec![1, 0] }
    }

    #[test]
    fn lines_surround_every_tile() {
        let image = render(&chart(), [0, 0, 255], true).unwrap();
        assert_eq!((image.width, image.height), (7, 4));
        let px = |x: usize, y: usize| &image.rgb[(y * 7 + x) * 3..(y * 7 + x) * 3 + 3];
        assert_eq!(px(0, 1), [0, 0, 255]);
        assert_eq!(px(1, 1), [200, 200, 200]);
        assert_eq!(px(3, 2), [0, 0, 255]);
        assert_eq!(px(4, 2), [10, 10, 10]);
        assert_eq!(px(4, 3), [0, 0, 255]);
    }

    #[test]
    fn tiles_only_packs_tiles() {
        let image = render(&chart(), [0, 0, 255], false).unwrap();
        assert_eq!((image.width, image.height), (4, 2));
        assert_eq!(image.rgb[..12], [200, 200, 200, 200, 200, 200, 10, 10, 10, 10, 10, 10]);
    }

    #[test]
    fn counts_add_up() {
        assert_eq!(chart().tile_counts(), [1, 1]);
    }

    #[test]
    fn settings_out_of_range_are_refused() {
        assert_eq!(ChartSettings::default().check(), Ok(()));
        let refused = |change: fn(&mut ChartSettings)| {
            let mut settings = ChartSettings::default();
            change(&mut settings);
            settings.check().unwrap_err().to_string()
        };
        assert_eq!(refused(|s| s.canvas_mm.0 = -5.0), "The canvas width must be between 1 and 10000 mm.");
        assert_eq!(refused(|s| s.canvas_mm.1 = f64::NAN), "The canvas height must be between 1 and 10000 mm.");
        assert_eq!(refused(|s| s.tile_mm = 0.0), "The tile size must be between 0.1 and 1000 mm.");
        assert_eq!(refused(|s| s.line_mm = 25.0), "The line width must be between 0 and 20 mm.");
        assert_eq!(refused(|s| s.dpi = 70_000), "The resolution must be between 72 and 2400 DPI.");
        assert_eq!(refused(|s| s.n_colours = 0), "The number of colours must be between 1 and 256.");
    }

    #[test]
    fn pixel_sizing_ignores_the_canvas() {
        let mut settings = ChartSettings { sizing: Sizing::Pixels, ..ChartSettings::default() };
        settings.canvas_mm = (-1.0, f64::NAN);
        assert_eq!(settings.check(), Ok(()));
        // A 3 x 2 picture drawn 5 times larger becomes 3 x 2 tiles, colours exact.
        let cells = [[200, 40, 40], [20, 20, 20], [200, 40, 40], [20, 20, 20], [250, 250, 250], [250, 250, 250]];
        let mut rgba = Vec::new();
        for y in 0..10 {
            for x in 0..15 {
                let [r, g, b] = cells[(y / 5) * 3 + x / 5];
                rgba.extend([r, g, b, 255]);
            }
        }
        let image = Image { width: 15, height: 10, rgba };
        let chart = build_chart(&image, &settings).unwrap();
        assert_eq!((chart.layout.tiles_x, chart.layout.tiles_y), (3, 2));
        assert_eq!(chart.palette, [[20, 20, 20], [200, 40, 40], [250, 250, 250]]);
        assert_eq!(chart.labels, [1, 0, 1, 0, 2, 2]);
    }

    fn four_paints() -> TileChart {
        let layout = GridLayout { tiles_x: 4, tiles_y: 1, tile_px: 2, line_px: 1, dpi: 300 };
        TileChart { layout, palette: vec![[0, 0, 0], [80, 0, 0], [160, 0, 0], [240, 0, 0]], labels: vec![0, 1, 2, 3] }
    }

    #[test]
    fn merging_moves_tiles_and_drops_the_paint() {
        let merged = merge_paints(&four_paints(), &[([80, 0, 0], [240, 0, 0])]);
        assert_eq!(merged.palette, [[0, 0, 0], [160, 0, 0], [240, 0, 0]]);
        assert_eq!(merged.labels, [0, 2, 1, 2]);
    }

    #[test]
    fn merges_chain_in_order() {
        let merged = merge_paints(&four_paints(), &[([0, 0, 0], [80, 0, 0]), ([80, 0, 0], [160, 0, 0])]);
        assert_eq!(merged.palette, [[160, 0, 0], [240, 0, 0]]);
        assert_eq!(merged.labels, [0, 0, 0, 1]);
    }

    #[test]
    fn merges_with_missing_colours_change_nothing() {
        let missing = [([1, 2, 3], [0, 0, 0]), ([0, 0, 0], [1, 2, 3]), ([80, 0, 0], [80, 0, 0])];
        assert_eq!(merge_paints(&four_paints(), &missing), four_paints());
        // Once merged away, a colour can neither take nor give tiles.
        let gone = [([0, 0, 0], [80, 0, 0]), ([160, 0, 0], [0, 0, 0]), ([0, 0, 0], [240, 0, 0])];
        assert_eq!(merge_paints(&four_paints(), &gone).labels, [0, 0, 1, 2]);
    }

    #[test]
    fn an_impossible_raster_is_refused_not_overflowed() {
        let mut huge = chart();
        huge.layout.tile_px = u32::MAX;
        assert!(render(&huge, [0, 0, 0], true).is_err());
    }
}
