//! Browser build of the Stencilator engine, called from the app's Web Worker.
//!
//! The worker decodes an image once into a [`Source`], builds a [`Chart`] from it for
//! each change of settings, and asks the chart for files. Files come back as bytes, and
//! errors as JavaScript `Error`s carrying the engine's message. Objects made here live in
//! WebAssembly memory until JavaScript calls `free()` on them.

use stencilator_core::chart::{ChartSettings, TileChart, build_chart};
use stencilator_core::colour::Rgb;
use stencilator_core::export::{ChartFormat, encode_chart};
use stencilator_core::quantise::ColourMode;
use stencilator_core::source::{Image, decode};
use stencilator_core::stencil::{RECOMMENDED_MIN_WEB_MM, stencil_file, stencil_filename, stencil_files};
use stencilator_core::zip::zip;
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// The engine version.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    stencilator_core::VERSION.to_owned()
}

/// Thinnest grid line, in millimetres, that makes a sturdy stencil web.
#[wasm_bindgen(js_name = recommendedMinWebMm)]
#[must_use]
pub fn recommended_min_web_mm() -> f64 {
    RECOMMENDED_MIN_WEB_MM
}

fn error(e: &stencilator_core::Error) -> JsError {
    JsError::new(e.message())
}

/// A colour as `0xRRGGBB`, the form JavaScript colour inputs convert to most easily.
fn rgb(hex: u32) -> Rgb {
    let [_, r, g, b] = hex.to_be_bytes();
    [r, g, b]
}

/// How colours are reduced.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Colour clustering.
    Colour = "colour",
    /// Shades of grey.
    Greyscale = "greyscale",
    /// Black and white only.
    BlackWhite = "black-white",
}

/// A chart file format.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Lossless raster.
    Png = "png",
    /// Raster at quality 95.
    Jpeg = "jpeg",
    /// Vector, at the exact size.
    Pdf = "pdf",
}

/// Chart settings, starting from the engine's defaults.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Canvas width in millimetres.
    #[wasm_bindgen(js_name = canvasWidthMm)]
    pub canvas_width_mm: f64,
    /// Canvas height in millimetres.
    #[wasm_bindgen(js_name = canvasHeightMm)]
    pub canvas_height_mm: f64,
    /// Side of a tile in millimetres.
    #[wasm_bindgen(js_name = tileMm)]
    pub tile_mm: f64,
    /// Width of a grid line in millimetres; 0 for none.
    #[wasm_bindgen(js_name = lineMm)]
    pub line_mm: f64,
    /// Device pixels per inch.
    pub dpi: u32,
    /// How colours are reduced.
    pub mode: Mode,
    /// Most colours in the palette.
    pub colours: u32,
    /// Colour that transparent parts take, as `0xRRGGBB`.
    pub background: u32,
    /// Seed for the colour clustering.
    pub seed: u32,
}

#[wasm_bindgen]
impl Settings {
    /// The defaults: a 200 by 200 mm canvas, 5 mm tiles, 0.8 mm lines, 300 DPI, up to
    /// 16 colours on white.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Settings {
        let d = ChartSettings::default();
        let [r, g, b] = d.background;
        Settings {
            canvas_width_mm: d.canvas_mm.0,
            canvas_height_mm: d.canvas_mm.1,
            tile_mm: d.tile_mm,
            line_mm: d.line_mm,
            dpi: d.dpi,
            mode: Mode::Colour,
            colours: u32::try_from(d.n_colours).unwrap_or(u32::MAX),
            background: u32::from_be_bytes([0, r, g, b]),
            seed: 0,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings::new()
    }
}

impl TryFrom<&Settings> for ChartSettings {
    type Error = JsError;

    fn try_from(s: &Settings) -> Result<ChartSettings, JsError> {
        Ok(ChartSettings {
            canvas_mm: (s.canvas_width_mm, s.canvas_height_mm),
            tile_mm: s.tile_mm,
            line_mm: s.line_mm,
            dpi: s.dpi,
            mode: match s.mode {
                Mode::Colour => ColourMode::Colour,
                Mode::Greyscale => ColourMode::Greyscale,
                Mode::BlackWhite => ColourMode::BlackWhite,
                // wasm-bindgen's stand-in for a string that is not one of the above.
                Mode::__Invalid => {
                    return Err(JsError::new("Unknown colour mode; use colour, greyscale or black-white."));
                }
            },
            n_colours: usize::try_from(s.colours).unwrap_or(usize::MAX),
            background: rgb(s.background),
            seed: u64::from(s.seed),
        })
    }
}

/// A decoded image, upright, as straight RGBA.
#[wasm_bindgen]
pub struct Source {
    image: Image,
}

#[wasm_bindgen]
impl Source {
    /// Decodes a PNG, JPEG or WebP file.
    ///
    /// # Errors
    ///
    /// When the bytes are not such an image, or it is too large.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<Source, JsError> {
        Ok(Source { image: decode(bytes).map_err(|e| error(&e))? })
    }

    /// Width in pixels.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn width(&self) -> u32 {
        self.image.width
    }

    /// Height in pixels.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn height(&self) -> u32 {
        self.image.height
    }

    /// Lays out the grid and reduces the colours.
    ///
    /// # Errors
    ///
    /// When a setting is out of range or the settings cannot produce a grid.
    pub fn chart(&self, settings: &Settings) -> Result<Chart, JsError> {
        Ok(Chart { chart: build_chart(&self.image, &settings.try_into()?).map_err(|e| error(&e))? })
    }
}

/// A tile grid with one palette colour per tile.
#[wasm_bindgen]
pub struct Chart {
    chart: TileChart,
}

#[wasm_bindgen]
#[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^53")]
impl Chart {
    /// Tiles across.
    #[wasm_bindgen(getter, js_name = tilesX)]
    #[must_use]
    pub fn tiles_x(&self) -> u32 {
        self.chart.layout.tiles_x
    }

    /// Tiles down.
    #[wasm_bindgen(getter, js_name = tilesY)]
    #[must_use]
    pub fn tiles_y(&self) -> u32 {
        self.chart.layout.tiles_y
    }

    /// Side of a tile in device pixels.
    #[wasm_bindgen(getter, js_name = tilePx)]
    #[must_use]
    pub fn tile_px(&self) -> u32 {
        self.chart.layout.tile_px
    }

    /// Width of a grid line in device pixels.
    #[wasm_bindgen(getter, js_name = linePx)]
    #[must_use]
    pub fn line_px(&self) -> u32 {
        self.chart.layout.line_px
    }

    /// Side of a tile in millimetres, after rounding to whole pixels.
    #[wasm_bindgen(getter, js_name = tileMm)]
    #[must_use]
    pub fn tile_mm(&self) -> f64 {
        self.chart.layout.tile_mm()
    }

    /// Width of a grid line in millimetres, after rounding to whole pixels.
    #[wasm_bindgen(getter, js_name = lineMm)]
    #[must_use]
    pub fn line_mm(&self) -> f64 {
        self.chart.layout.line_mm()
    }

    /// Width of the whole chart in millimetres.
    #[wasm_bindgen(getter, js_name = widthMm)]
    #[must_use]
    pub fn width_mm(&self) -> f64 {
        self.chart.layout.width_mm()
    }

    /// Height of the whole chart in millimetres.
    #[wasm_bindgen(getter, js_name = heightMm)]
    #[must_use]
    pub fn height_mm(&self) -> f64 {
        self.chart.layout.height_mm()
    }

    /// Width of the whole chart in device pixels.
    #[wasm_bindgen(getter, js_name = widthPx)]
    #[must_use]
    pub fn width_px(&self) -> f64 {
        self.chart.layout.width_px() as f64
    }

    /// Height of the whole chart in device pixels.
    #[wasm_bindgen(getter, js_name = heightPx)]
    #[must_use]
    pub fn height_px(&self) -> f64 {
        self.chart.layout.height_px() as f64
    }

    /// The palette as RGB triples, darkest first.
    #[must_use]
    pub fn palette(&self) -> Vec<u8> {
        self.chart.palette.concat()
    }

    /// The palette index of every tile, row by row from the top.
    #[must_use]
    pub fn labels(&self) -> Vec<u32> {
        self.chart.labels.clone()
    }

    /// Number of tiles of each palette colour.
    #[wasm_bindgen(js_name = tileCounts)]
    #[must_use]
    pub fn tile_counts(&self) -> Vec<u32> {
        self.chart.tile_counts().into_iter().map(|n| u32::try_from(n).unwrap_or(u32::MAX)).collect()
    }

    /// The chart as a file, with grid lines in `grid_colour` (`0xRRGGBB`), or with the
    /// tiles packed edge to edge.
    ///
    /// # Errors
    ///
    /// When the chart is too large for the format.
    #[wasm_bindgen(js_name = chartFile)]
    pub fn chart_file(&self, format: Format, grid_colour: u32, with_lines: bool) -> Result<Vec<u8>, JsError> {
        let format = match format {
            Format::Png => ChartFormat::Png,
            Format::Jpeg => ChartFormat::Jpeg,
            Format::Pdf => ChartFormat::Pdf,
            Format::__Invalid => return Err(JsError::new("Unknown chart format; use png, jpeg or pdf.")),
        };
        encode_chart(&self.chart, format, rgb(grid_colour), with_lines).map_err(|e| error(&e))
    }

    /// The file name of each colour's stencil, in palette order.
    #[wasm_bindgen(js_name = stencilNames)]
    #[must_use]
    pub fn stencil_names(&self) -> Vec<String> {
        self.chart.palette.iter().map(|&c| stencil_filename(c)).collect()
    }

    /// One colour's stencil as binary STL.
    ///
    /// # Errors
    ///
    /// When the grid has no lines, the thickness is out of range, or there is no such
    /// colour.
    pub fn stencil(&self, index: u32, thickness_mm: f64) -> Result<Vec<u8>, JsError> {
        Ok(stencil_file(&self.chart, index, thickness_mm).map_err(|e| error(&e))?.1)
    }

    /// Every colour's stencil, as STL files in one zip.
    ///
    /// # Errors
    ///
    /// When the grid has no lines or the thickness is out of range.
    #[wasm_bindgen(js_name = stencilsZip)]
    pub fn stencils_zip(&self, thickness_mm: f64) -> Result<Vec<u8>, JsError> {
        let files = stencil_files(&self.chart, thickness_mm).map_err(|e| error(&e))?;
        zip(&files).map_err(|e| error(&e))
    }
}
