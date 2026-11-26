//! Reading PNG, JPEG and WebP images from bytes.
//!
//! Images come out upright (EXIF orientation applied) as straight RGBA. 16-bit greyscale,
//! with or without transparency, is scaled to 8 bits from the bit depth its values
//! actually use, so 12-bit data in a 16-bit file is not left nearly black.

use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

use crate::Error;

/// Largest image accepted, in pixels: Pillow's own limit against decompression bombs.
pub const MAX_IMAGE_PIXELS: u64 = 178_956_970;

/// Largest values of 8-, 10-, 12-, 14- and 16-bit data, which often sits in 16-bit files.
const INTEGER_CEILINGS: [u16; 5] = [255, 1023, 4095, 16383, 65535];

/// A decoded image: straight (not premultiplied) RGBA, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Four bytes per pixel.
    pub rgba: Vec<u8>,
}

impl Image {
    /// One pixel's RGBA.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }
}

/// Decodes a PNG, JPEG or WebP image.
///
/// # Errors
///
/// [`Error::Image`] when the bytes are not an image in one of those formats, or the image
/// is larger than [`MAX_IMAGE_PIXELS`].
pub fn decode(bytes: &[u8]) -> Result<Image, Error> {
    let unreadable = |e: image::ImageError| Error::Image(format!("The image could not be read: {e}."));
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| unreadable(e.into()))?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(4 * MAX_IMAGE_PIXELS + (64 << 20));
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(unreadable)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err(Error::Image(format!(
            "The image is {width} x {height} pixels; the limit is {} megapixels.",
            MAX_IMAGE_PIXELS / 1_000_000
        )));
    }
    let orientation = decoder.orientation().map_err(unreadable)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(unreadable)?;
    image.apply_orientation(orientation);
    let (width, height) = (image.width(), image.height());
    let rgba = match image {
        DynamicImage::ImageLuma16(grey) => {
            grey_to_8bit(grey.as_raw()).into_iter().flat_map(|v| [v, v, v, 255]).collect()
        }
        DynamicImage::ImageLumaA16(grey) => {
            let (luma, alpha): (Vec<u16>, Vec<u16>) =
                grey.as_raw().as_chunks::<2>().0.iter().map(|&[l, a]| (l, a)).unzip();
            grey_to_8bit(&luma).into_iter().zip(alpha).flat_map(|(v, a)| [v, v, v, alpha_to_8bit(a)]).collect()
        }
        other => other.into_rgba8().into_raw(),
    };
    Ok(Image { width, height, rgba })
}

/// Scales 16-bit greyscale to 8 bits from the depth the values use (8, 10, 12, 14 or 16
/// bits), rounding halves to even.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "rounded and clamped to 0..=255")]
fn grey_to_8bit(values: &[u16]) -> Vec<u8> {
    let high = values.iter().copied().max().unwrap_or(0);
    let ceiling = INTEGER_CEILINGS.iter().copied().find(|&c| high <= c).unwrap_or(u16::MAX);
    let scale = 255.0 / f64::from(ceiling);
    values.iter().map(|&v| (f64::from(v) * scale).round_ties_even().clamp(0.0, 255.0) as u8).collect()
}

/// A 16-bit alpha as 8 bits, rounded.
#[allow(clippy::cast_possible_truncation, reason = "at most 255")]
fn alpha_to_8bit(alpha: u16) -> u8 {
    ((u32::from(alpha) * 255 + 32_767) / 65_535) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(image: &DynamicImage) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn twelve_bit_grey_fills_the_range() {
        let grey = image::ImageBuffer::from_raw(3, 1, vec![0_u16, 2048, 4095]).unwrap();
        let image = decode(&png(&DynamicImage::ImageLuma16(grey))).unwrap();
        assert_eq!(
            [image.pixel(0, 0), image.pixel(1, 0), image.pixel(2, 0)],
            [[0, 0, 0, 255], [128, 128, 128, 255], [255; 4]]
        );
    }

    #[test]
    fn twelve_bit_grey_with_transparency_fills_the_range() {
        let grey = image::ImageBuffer::from_raw(3, 1, vec![0_u16, 65_535, 2048, 65_535, 4095, 0]).unwrap();
        let image = decode(&png(&DynamicImage::ImageLumaA16(grey))).unwrap();
        assert_eq!(
            [image.pixel(0, 0), image.pixel(1, 0), image.pixel(2, 0)],
            [[0, 0, 0, 255], [128, 128, 128, 255], [255, 255, 255, 0]]
        );
    }

    #[test]
    fn rgba_comes_through_unchanged() {
        let rgba = image::RgbaImage::from_raw(2, 1, vec![1, 2, 3, 4, 250, 251, 252, 0]).unwrap();
        let image = decode(&png(&DynamicImage::ImageRgba8(rgba))).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.rgba, [1, 2, 3, 4, 250, 251, 252, 0]);
    }

    #[test]
    fn rubbish_is_refused() {
        assert!(matches!(decode(b"not an image"), Err(Error::Image(_))));
    }
}
