//! Charts as PNG, JPEG or vector PDF at their physical size.
//!
//! Raster files carry the DPI in their metadata so they print at the intended size. The
//! PDF is drawn as filled rectangles, so it stays sharp and small at any size.

use std::fmt::Write as _;

use crate::Error;
use crate::chart::{TileChart, render};
use crate::colour::Rgb;
use crate::text::thousands;

const POINTS_PER_INCH: f64 = 72.0;
const JPEG_MAX_SIDE_PX: u32 = 65_535;
const JPEG_QUALITY: u8 = 95;
/// 200 inches, the page size limit most PDF readers enforce.
const PDF_MAX_SIDE_PT: f64 = 14_400.0;

/// A chart file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartFormat {
    /// Lossless raster with DPI metadata.
    Png,
    /// Raster at quality 95 without chroma subsampling, with DPI metadata.
    Jpeg,
    /// Vector rectangles at the exact physical size.
    Pdf,
}

impl ChartFormat {
    /// The format a file name asks for, by its extension.
    ///
    /// # Errors
    ///
    /// [`Error::Export`] for any other extension.
    pub fn from_file_name(name: &str) -> Result<ChartFormat, Error> {
        // The last path component, and a dot that does not start it (".png" is a hidden
        // file with no extension).
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let suffix = base.rfind('.').filter(|&i| i > 0).map(|i| base[i..].to_ascii_lowercase());
        match suffix.as_deref() {
            Some(".png") => Ok(ChartFormat::Png),
            Some(".jpg" | ".jpeg") => Ok(ChartFormat::Jpeg),
            Some(".pdf") => Ok(ChartFormat::Pdf),
            other => Err(Error::Export(format!(
                "Unsupported file type {}; use PNG, JPEG or PDF.",
                other.unwrap_or("(none)")
            ))),
        }
    }
}

/// The chart as a file, with grid lines or with the tiles packed edge to edge.
///
/// # Errors
///
/// [`Error::Export`] when the chart is too large for the format.
pub fn encode_chart(
    chart: &TileChart,
    format: ChartFormat,
    grid_colour: Rgb,
    with_lines: bool,
) -> Result<Vec<u8>, Error> {
    match format {
        ChartFormat::Pdf => chart_pdf(chart, grid_colour, with_lines),
        ChartFormat::Png => {
            let image = render(chart, grid_colour, with_lines)?;
            encode_png(image.width, image.height, &image.rgb, chart.layout.dpi)
        }
        ChartFormat::Jpeg => {
            let layout = if with_lines { chart.layout } else { chart.layout.without_lines() };
            if layout.width_px().max(layout.height_px()) > u64::from(JPEG_MAX_SIDE_PX) {
                return Err(Error::Export(format!(
                    "JPEG is limited to {} px per side; save as PNG or PDF, or lower the DPI.",
                    thousands(u64::from(JPEG_MAX_SIDE_PX))
                )));
            }
            let image = render(chart, grid_colour, with_lines)?;
            encode_jpeg(image.width, image.height, &image.rgb, chart.layout.dpi)
        }
    }
}

/// RGB pixels as PNG, with the resolution in its `pHYs` chunk as Pillow writes it.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "positive and far below 2^32")]
fn encode_png(width: u32, height: u32, rgb: &[u8], dpi: u32) -> Result<Vec<u8>, Error> {
    let failed = |e: png::EncodingError| Error::Export(format!("The PNG could not be written: {e}."));
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let per_metre = (f64::from(dpi) / 0.0254 + 0.5) as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions { xppu: per_metre, yppu: per_metre, unit: png::Unit::Meter }));
    let mut writer = encoder.write_header().map_err(failed)?;
    writer.write_image_data(rgb).map_err(failed)?;
    writer.finish().map_err(failed)?;
    Ok(out)
}

/// RGB pixels as JPEG. 4:4:4 chroma keeps tile edges clean.
fn encode_jpeg(width: u32, height: u32, rgb: &[u8], dpi: u32) -> Result<Vec<u8>, Error> {
    let side = |v: u32| u16::try_from(v).map_err(|_| Error::Export("The JPEG is too large.".to_owned()));
    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
    let dpi = u16::try_from(dpi)
        .map_err(|_| Error::Export("A JPEG cannot record more than 65,535 DPI; save as PNG or PDF.".to_owned()))?;
    encoder
        .set_density(jpeg_encoder::PixelDensity { density: (dpi, dpi), unit: jpeg_encoder::PixelDensityUnit::Inches });
    encoder
        .encode(rgb, side(width)?, side(height)?, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| Error::Export(format!("The JPEG could not be written: {e}.")))?;
    Ok(out)
}

fn rgb_operator(colour: Rgb) -> String {
    let [r, g, b] = colour.map(|c| f64::from(c) / 255.0);
    format!("{r:.4} {g:.4} {b:.4} rg")
}

/// `(start, length)` of the true cells, merging neighbours when tiles touch.
fn runs(mask: impl Iterator<Item = bool>, merge: bool) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for (i, on) in (0..).zip(mask) {
        if !on {
            continue;
        }
        match runs.last_mut() {
            Some((start, length)) if merge && *start + *length == i => *length += 1,
            _ => runs.push((i, 1)),
        }
    }
    runs
}

/// A single-page PDF the size of the chart, drawn with filled rectangles.
///
/// # Errors
///
/// [`Error::Export`] when a side would exceed 200 inches.
#[allow(clippy::cast_precision_loss, reason = "pixel counts are far below 2^52")]
pub fn chart_pdf(chart: &TileChart, grid_colour: Rgb, with_lines: bool) -> Result<Vec<u8>, Error> {
    let layout = if with_lines { chart.layout } else { chart.layout.without_lines() };
    let scale = POINTS_PER_INCH / f64::from(layout.dpi);
    let page_w = layout.width_px() as f64 * scale;
    let page_h = layout.height_px() as f64 * scale;
    if page_w.max(page_h) > PDF_MAX_SIDE_PT {
        return Err(Error::Export("PDF pages are limited to 200 inches (508 cm) per side.".to_owned()));
    }
    let tile = f64::from(layout.tile_px) * scale;
    let (line, pitch) = (u64::from(layout.line_px), layout.pitch_px());

    let mut ops: Vec<String> = Vec::new();
    if layout.line_px > 0 {
        ops.push(rgb_operator(grid_colour));
        ops.push(format!("0 0 {page_w:.4} {page_h:.4} re f"));
    }
    for (index, &colour) in (0..).zip(&chart.palette) {
        ops.push(rgb_operator(colour));
        for row in 0..layout.tiles_y {
            // PDF's origin is the bottom-left corner; image row 0 is at the top.
            let y = page_h - (line + u64::from(row) * pitch) as f64 * scale - tile;
            let mask = (0..layout.tiles_x).map(|x| chart.label(x, row) == index);
            for (start, length) in runs(mask, layout.line_px == 0) {
                let x = (line + u64::from(start) * pitch) as f64 * scale;
                ops.push(format!("{x:.4} {y:.4} {:.4} {tile:.4} re", f64::from(length) * tile));
            }
        }
        ops.push("f".to_owned());
    }
    Ok(single_page_pdf(page_w, page_h, ops.join("\n").as_bytes()))
}

fn single_page_pdf(width_pt: f64, height_pt: f64, content: &[u8]) -> Vec<u8> {
    let stream = miniz_oxide::deflate::compress_to_vec_zlib(content, 9);
    let mut content_object = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", stream.len()).into_bytes();
    content_object.extend_from_slice(&stream);
    content_object.extend_from_slice(b"\nendstream");
    let objects: [Vec<u8>; 5] = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width_pt:.4} {height_pt:.4}] \
             /Resources << >> /Contents 4 0 R >>"
        )
        .into_bytes(),
        content_object,
        b"<< /Producer (Stencilator) >>".to_vec(),
    ];
    let mut out = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (number, body) in (1..).zip(&objects) {
        offsets.push(out.len());
        out.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = out.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(tail, "{offset:010} 00000 n ");
    }
    let _ = write!(
        tail,
        "trailer\n<< /Size {} /Root 1 0 R /Info {} 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1,
        objects.len()
    );
    out.extend_from_slice(tail.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::GridLayout;

    fn chart() -> TileChart {
        let layout = GridLayout { tiles_x: 3, tiles_y: 2, tile_px: 30, line_px: 5, dpi: 300 };
        TileChart { layout, palette: vec![[0, 0, 0], [250, 250, 250]], labels: vec![0, 1, 1, 1, 0, 0] }
    }

    #[test]
    fn formats_follow_the_extension() {
        assert_eq!(ChartFormat::from_file_name("chart.PNG").unwrap(), ChartFormat::Png);
        assert_eq!(ChartFormat::from_file_name("a.b/chart.jpeg").unwrap(), ChartFormat::Jpeg);
        assert_eq!(ChartFormat::from_file_name("chart.pdf").unwrap(), ChartFormat::Pdf);
        assert_eq!(
            ChartFormat::from_file_name("chart.gif").unwrap_err().message(),
            "Unsupported file type .gif; use PNG, JPEG or PDF."
        );
        assert_eq!(
            ChartFormat::from_file_name("a.b/chart").unwrap_err().message(),
            "Unsupported file type (none); use PNG, JPEG or PDF."
        );
        assert!(ChartFormat::from_file_name("out/.png").is_err());
        assert!(ChartFormat::from_file_name("chart.png/").is_err());
    }

    #[test]
    fn png_and_jpeg_decode_to_the_chart_size_with_dpi() {
        for format in [ChartFormat::Png, ChartFormat::Jpeg] {
            let bytes = encode_chart(&chart(), format, [9, 9, 9], true).unwrap();
            let image = image::load_from_memory(&bytes).unwrap();
            assert_eq!((image.width(), image.height()), (110, 75), "{format:?}");
        }
        let png = encode_chart(&chart(), ChartFormat::Png, [9, 9, 9], true).unwrap();
        // pHYs: 11811 pixels per metre is 300 DPI.
        assert!(png.windows(8).any(|w| w == [b'p', b'H', b'Y', b's', 0, 0, 0x2e, 0x23]));
    }

    #[test]
    fn neighbouring_tiles_merge_only_without_lines() {
        assert_eq!(runs([true, true, false, true].into_iter(), true), [(0, 2), (3, 1)]);
        assert_eq!(runs([true, true, false, true].into_iter(), false), [(0, 1), (1, 1), (3, 1)]);
    }

    #[test]
    fn pdf_is_well_formed() {
        let pdf = chart_pdf(&chart(), [9, 9, 9], true).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.7\n"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&pdf);
        let xref: usize = text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        assert!(pdf[xref..].starts_with(b"xref\n"));
    }
}
