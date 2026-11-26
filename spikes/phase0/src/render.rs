//! SVG to pixels with usvg and resvg, using only the fonts passed in.

use std::sync::Arc;

use resvg::tiny_skia;
use usvg::fontdb;

/// SVG user units (CSS pixels) per millimetre.
pub const SVG_PX_PER_MM: f32 = 96.0 / 25.4;

/// Rasters larger than this are refused.
const MAX_PIXELS: u64 = 120_000_000;

/// Premultiplied RGBA pixels, rows top to bottom.
#[derive(Debug, Clone)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Raster {
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    /// The pixels composited onto white, as plain RGB (for viewing).
    #[must_use]
    pub fn on_white(&self) -> Vec<u8> {
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let a = u16::from(p[3]);
                [0, 1, 2].map(|c| (u16::from(p[c]) + (255 - a)) as u8)
            })
            .collect()
    }
}

/// Parses an SVG with no system fonts: text uses the given font files only.
pub fn parse(svg: &str, fonts: &[&[u8]]) -> Result<usvg::Tree, String> {
    let mut db = fontdb::Database::new();
    for font in fonts {
        db.load_font_data(font.to_vec());
    }
    let mut options = usvg::Options::default();
    let first_family = db.faces().next().and_then(|face| face.families.first()).map(|(name, _)| name.clone());
    if let Some(family) = first_family {
        db.set_serif_family(family.clone());
        db.set_sans_serif_family(family.clone());
        db.set_monospace_family(family.clone());
        db.set_cursive_family(family.clone());
        db.set_fantasy_family(family.clone());
        options.font_family = family;
    }
    options.fontdb = Arc::new(db);
    usvg::Tree::from_str(svg, &options).map_err(|error| format!("SVG: {error}"))
}

/// Renders the whole drawing at the given resolution.
pub fn render(tree: &usvg::Tree, px_per_mm: f32) -> Result<Raster, String> {
    let scale = px_per_mm / SVG_PX_PER_MM;
    let size = tree.size();
    let width = (size.width() * scale).ceil() as u32;
    let height = (size.height() * scale).ceil() as u32;
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(format!("{width} x {height} pixels is too large"));
    }
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(|| "the drawing is empty".to_owned())?;
    resvg::render(tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Ok(Raster { width, height, rgba: pixmap.take() })
}

/// Size of the drawing in millimetres.
#[must_use]
pub fn size_mm(tree: &usvg::Tree) -> (f32, f32) {
    let size = tree.size();
    (size.width() / SVG_PX_PER_MM, size.height() / SVG_PX_PER_MM)
}
