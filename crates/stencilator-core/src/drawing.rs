//! Vector drawings and typed text: SVG read with usvg and rendered with resvg.
//!
//! Text uses only the font files the shell passes in, never the system's, so the app, the
//! command line and the MCP server lay it out the same way. Links in the SVG to other files
//! are ignored: only images embedded in the drawing itself are read.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::sync::Arc;

use resvg::tiny_skia;
use usvg::{Group, ImageHrefResolver, Node, Paint, fontdb};

use crate::Error;
use crate::colour::Rgb;
use crate::source::Image;

/// SVG user units (CSS pixels) per millimetre.
const PX_PER_MM: f64 = 96.0 / 25.4;

/// Space between lines of text, in multiples of the font size.
const LINE_SPACING: f64 = 1.2;

/// A parsed drawing and the part of it that is the artwork, in user units.
pub struct Drawing {
    tree: usvg::Tree,
    origin: (f32, f32),
    size: (f32, f32),
}

impl std::fmt::Debug for Drawing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drawing").field("origin", &self.origin).field("size", &self.size).finish_non_exhaustive()
    }
}

impl Drawing {
    /// Width and height in millimetres.
    pub fn size_mm(&self) -> (f64, f64) {
        (f64::from(self.size.0) / PX_PER_MM, f64::from(self.size.1) / PX_PER_MM)
    }

    /// The colours the drawing paints with, when every fill and stroke is one solid,
    /// opaque colour; `None` when anything mixes colours (gradients, patterns, embedded
    /// images, transparency, masks or filters). Clipping only cuts, so it is allowed.
    pub fn solid_paints(&self) -> Option<Vec<Rgb>> {
        let mut paints = BTreeSet::new();
        solid(self.tree.root(), &mut paints).then(|| paints.into_iter().collect())
    }

    /// The drawing rendered onto `width` by `height` cells: straight RGBA, rows from the
    /// top, transparent where nothing is drawn. Edges are not anti-aliased.
    ///
    /// # Errors
    ///
    /// [`Error::Image`] when there is nothing to render onto.
    #[allow(clippy::cast_precision_loss, reason = "at most 16 million cells a side, exact in f32")]
    pub fn render(&self, width: u32, height: u32) -> Result<Image, Error> {
        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| Error::Image("The drawing is too large to draw.".to_owned()))?;
        let (sx, sy) = (width as f32 / self.size.0, height as f32 / self.size.1);
        let transform = tiny_skia::Transform::from_row(sx, 0.0, 0.0, sy, -self.origin.0 * sx, -self.origin.1 * sy);
        resvg::render(&self.tree, transform, &mut pixmap.as_mut());
        let mut rgba = pixmap.take();
        // tiny-skia keeps colours premultiplied by alpha; the engine works in straight RGBA.
        for p in rgba.as_chunks_mut::<4>().0 {
            let alpha = u32::from(p[3]);
            if alpha != 0 && alpha != 255 {
                for c in &mut p[..3] {
                    *c = u8::try_from(((u32::from(*c) * 255 + alpha / 2) / alpha).min(255)).unwrap_or(u8::MAX);
                }
            }
        }
        Ok(Image { width, height, rgba })
    }
}

fn solid(group: &Group, paints: &mut BTreeSet<Rgb>) -> bool {
    if group.opacity().get() < 1.0 || group.mask().is_some() || !group.filters().is_empty() {
        return false;
    }
    group.children().iter().all(|node| match node {
        Node::Group(child) => solid(child, paints),
        Node::Text(text) => solid(text.flattened(), paints),
        Node::Image(_) => false,
        Node::Path(path) => {
            if !path.is_visible() {
                return true;
            }
            let fill = path.fill().map(|f| (f.paint(), f.opacity()));
            let stroke = path.stroke().map(|s| (s.paint(), s.opacity()));
            [fill, stroke].into_iter().flatten().all(|(paint, opacity)| match paint {
                Paint::Color(c) if opacity.get() >= 1.0 => {
                    paints.insert([c.red, c.green, c.blue]);
                    true
                }
                _ => false,
            })
        }
    })
}

/// Options that read nothing but the drawing and the given fonts.
fn options(fonts: &[&[u8]]) -> Result<usvg::Options<'static>, Error> {
    let mut db = fontdb::Database::new();
    for font in fonts {
        let before = db.len();
        db.load_font_data(font.to_vec());
        if db.len() == before {
            return Err(Error::Image("The font could not be read. Use a TrueType or OpenType file.".to_owned()));
        }
    }
    let mut options = usvg::Options {
        image_href_resolver: ImageHrefResolver {
            resolve_data: ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        // Without anti-aliasing each cell is the paint at its centre, so edges between
        // paints make no blends of the two.
        shape_rendering: usvg::ShapeRendering::CrispEdges,
        text_rendering: usvg::TextRendering::OptimizeSpeed,
        ..usvg::Options::default()
    };
    // Every generic family, and text that names a font the drawing does not have, uses the
    // first font given.
    let first = db.faces().next().and_then(|face| face.families.first()).map(|(name, _)| name.clone());
    if let Some(family) = first {
        db.set_serif_family(family.clone());
        db.set_sans_serif_family(family.clone());
        db.set_monospace_family(family.clone());
        db.set_cursive_family(family.clone());
        db.set_fantasy_family(family.clone());
        options.font_family = family;
    }
    options.fontdb = Arc::new(db);
    Ok(options)
}

/// Reads an SVG file. Its own width and height, in any unit, give its size. Text in it is
/// set in the given fonts, the first standing in for any it names; with no fonts, text is
/// left out.
///
/// # Errors
///
/// [`Error::Image`] when it is not an SVG that can be read, or a font cannot be read.
pub fn parse_svg(svg: &[u8], fonts: &[&[u8]]) -> Result<Drawing, Error> {
    let tree = usvg::Tree::from_data(svg, &options(fonts)?)
        .map_err(|e| Error::Image(format!("The SVG could not be read: {e}.")))?;
    let size = tree.size();
    Ok(Drawing { origin: (0.0, 0.0), size: (size.width(), size.height()), tree })
}

/// Lays out `text` as a drawing in one colour, centred line by line, `font_size_mm` being
/// the size of the font (the height of its em, a little more than a capital letter). The
/// drawing is as large as the text, with nothing around it.
///
/// # Errors
///
/// [`Error::Image`] when there is no text, no font or the font has none of the letters.
pub fn text_drawing(text: &str, fonts: &[&[u8]], font_size_mm: f64, colour: Rgb) -> Result<Drawing, Error> {
    if text.trim().is_empty() {
        return Err(Error::Image("There is no text to cut.".to_owned()));
    }
    if fonts.is_empty() {
        return Err(Error::Image("Text needs a font.".to_owned()));
    }
    if !(1.0..=1000.0).contains(&font_size_mm) {
        return Err(Error::Settings("The text size must be between 1 and 1000 mm.".to_owned()));
    }
    let [r, g, b] = colour;
    let size = font_size_mm * PX_PER_MM;
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><text font-size="{size}" fill="#{r:02x}{g:02x}{b:02x}" text-anchor="middle">"##
    );
    for (i, line) in text.lines().enumerate() {
        let line = line.trim_end();
        // An empty line still takes up its height.
        let line = if line.is_empty() { "\u{a0}".to_owned() } else { escape(line) };
        let dy = if i == 0 { 0.0 } else { LINE_SPACING };
        let _ = write!(svg, r#"<tspan x="0" dy="{dy}em">{line}</tspan>"#);
    }
    svg.push_str("</text></svg>");
    let tree = usvg::Tree::from_str(&svg, &options(fonts)?)
        .map_err(|e| Error::Image(format!("The text could not be laid out: {e}.")))?;
    // The bounds of the letters themselves: a text element's own box includes the space
    // above and below them that the font reserves.
    let bounds = tree
        .root()
        .children()
        .iter()
        .find_map(|node| match node {
            Node::Text(text) => text.flattened().abs_bounding_box(),
            _ => None,
        })
        .filter(|b| b.width() > 0.0 && b.height() > 0.0)
        .ok_or_else(|| Error::Image("The font has none of these letters.".to_owned()))?;
    Ok(Drawing { origin: (bounds.x(), bounds.y()), size: (bounds.width(), bounds.height()), tree })
}

fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &[u8] = include_bytes!("../../../fonts/NotoSans-Bold.ttf");
    const BADGE: &[u8] = include_bytes!("../../../fixtures/svg/badge.svg");
    const MIXED: &[u8] = include_bytes!("../../../fixtures/svg/clip-mask-text.svg");

    #[test]
    fn a_drawing_in_flat_colours_names_its_paints() {
        let badge = parse_svg(BADGE, &[FONT]).unwrap();
        let (w, h) = badge.size_mm();
        assert!((w - 70.0).abs() < 1e-4 && (h - 70.0).abs() < 1e-4, "{w} {h}");
        assert_eq!(badge.solid_paints(), Some(vec![[0x1d, 0x2f, 0x5c], [0xef, 0x7d, 0x2d]]));
        let image = badge.render(70, 70).unwrap();
        // The centre is orange, a corner is empty.
        assert_eq!(image.pixel(35, 30), [0xef, 0x7d, 0x2d, 255]);
        assert_eq!(image.pixel(0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn masks_and_gradients_mix_colours() {
        assert_eq!(parse_svg(MIXED, &[FONT]).unwrap().solid_paints(), None);
    }

    #[test]
    fn text_is_as_large_as_its_letters() {
        let text = text_drawing("HO", &[FONT], 20.0, [0, 0, 0]).unwrap();
        let (w, h) = text.size_mm();
        // In Noto Sans Bold, H reaches 714 thousandths of an em up and O runs from 10 below
        // the baseline to 725 above (read with fontTools); each is about 0.75 em wide.
        assert!((h - 0.735 * 20.0).abs() < 0.01, "{h}");
        assert!((25.0..32.0).contains(&w), "{w}");
        assert_eq!(text.solid_paints(), Some(vec![[0, 0, 0]]));
        // The second line sits 1.2 em lower.
        let two = text_drawing("HO\nHO", &[FONT], 20.0, [0, 0, 0]).unwrap();
        assert!((two.size_mm().1 - (0.735 + 1.2) * 20.0).abs() < 0.01, "{:?}", two.size_mm());
    }

    fn has_image(group: &Group) -> bool {
        group.children().iter().any(|node| match node {
            Node::Image(_) => true,
            Node::Group(child) => has_image(child),
            _ => false,
        })
    }

    #[test]
    fn nothing_is_read_from_elsewhere() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/pixel-art/fox.png");
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <image href="{path}" width="10" height="10"/><rect width="5" height="5" fill="red"/></svg>"#
        );
        // usvg on its own reads the file...
        assert!(has_image(usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap().root()));
        // ...the engine does not.
        assert_eq!(parse_svg(svg.as_bytes(), &[]).unwrap().solid_paints(), Some(vec![[255, 0, 0]]));
    }

    #[test]
    fn images_inside_the_drawing_are_drawn() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><image width="4" height="4"
            href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAD0lEQVR4nGNgaGAAIQgFAA4OAgFhGn2EAAAAAElFTkSuQmCC"/></svg>"#;
        let drawing = parse_svg(svg, &[]).unwrap();
        assert_eq!(drawing.solid_paints(), None);
        assert_eq!(drawing.render(4, 4).unwrap().pixel(1, 2), [0, 128, 0, 255]);
    }

    #[test]
    fn mistakes_are_explained() {
        assert_eq!(
            parse_svg(b"<nope", &[]).unwrap_err().to_string().split(':').next(),
            Some("The SVG could not be read")
        );
        assert_eq!(text_drawing("  ", &[FONT], 20.0, [0; 3]).unwrap_err().to_string(), "There is no text to cut.");
        assert_eq!(text_drawing("A", &[], 20.0, [0; 3]).unwrap_err().to_string(), "Text needs a font.");
        assert_eq!(
            text_drawing("A", &[b"not a font"], 20.0, [0; 3]).unwrap_err().to_string(),
            "The font could not be read. Use a TrueType or OpenType file."
        );
        assert_eq!(text_drawing("&<>", &[FONT], 20.0, [0; 3]).map(|d| d.size_mm().0 > 0.0).ok(), Some(true));
    }
}
