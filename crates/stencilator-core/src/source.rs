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

/// The factor by which pixel art has been scaled up: the largest whole number `n` for
/// which the image is made of `n` by `n` blocks of one colour each. 1 for most pictures.
///
/// It is the greatest common divisor of the width, the height and every place where a row
/// or a column changes colour. Fully transparent pixels count as one colour, whatever
/// their RGB values, since editors store those differently.
pub fn pixel_scale(image: &Image) -> u32 {
    let key = |p: &[u8]| if p[3] == 0 { 0 } else { u32::from_le_bytes([p[0], p[1], p[2], p[3]]) };
    let row_bytes = image.width as usize * 4;
    let row = |y: u32| &image.rgba[y as usize * row_bytes..(y as usize + 1) * row_bytes];
    let mut scale = gcd(image.width, image.height);
    for y in 0..image.height {
        let pixels = row(y).as_chunks::<4>().0;
        for (x, pair) in (1..).zip(pixels.windows(2)) {
            if scale == 1 {
                return 1;
            }
            if key(&pair[0]) != key(&pair[1]) {
                scale = gcd(scale, x);
            }
        }
    }
    for y in 1..image.height {
        if scale == 1 {
            return 1;
        }
        let (above, here) = (row(y - 1).as_chunks::<4>().0, row(y).as_chunks::<4>().0);
        if above.iter().zip(here).any(|(a, b)| key(a) != key(b)) {
            scale = gcd(scale, y);
        }
    }
    scale
}

/// The picture's own pixel grid: its size, or for pixel art scaled up by a whole factor
/// (see [`pixel_scale`]), the size it was drawn at.
pub fn pixel_grid(image: &Image) -> (u32, u32) {
    let scale = pixel_scale(image).max(1);
    (image.width / scale, image.height / scale)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
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

    /// `cells` (one colour per byte) drawn `scale` times larger.
    fn scaled(cells: &[&[u8]], scale: u32) -> Image {
        let colour = |c: u8| match c {
            b'.' => [0, 0, 0, 0],
            b'o' => [232, 119, 46, 255],
            _ => [43, 35, 32, 255],
        };
        let (w, h) = (u32::try_from(cells[0].len()).unwrap() * scale, u32::try_from(cells.len()).unwrap() * scale);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend(colour(cells[(y / scale) as usize][(x / scale) as usize]));
            }
        }
        Image { width: w, height: h, rgba }
    }

    #[test]
    fn scaled_up_pixel_art_is_found_at_its_own_size() {
        let art: &[&[u8]] = &[b".ok", b"oko", b"k.."];
        for scale in [1, 2, 3, 7, 20] {
            let image = scaled(art, scale);
            assert_eq!(pixel_scale(&image), scale);
            assert_eq!(pixel_grid(&image), (3, 3));
        }
    }

    #[test]
    fn one_odd_pixel_makes_every_pixel_a_tile() {
        let mut image = scaled(&[b"ok", b"ko"], 4);
        image.rgba[4 * 5] = 1;
        assert_eq!(pixel_scale(&image), 1);
        assert_eq!(pixel_grid(&image), (8, 8));
    }

    #[test]
    fn transparent_pixels_match_whatever_their_colour() {
        let mut image = scaled(&[b".o", b"o."], 2);
        image.rgba[0..4].copy_from_slice(&[255, 0, 0, 0]);
        assert_eq!(pixel_scale(&image), 2);
    }

    #[test]
    fn a_plain_image_is_one_tile() {
        let image = scaled(&[b"oo", b"oo"], 6);
        assert_eq!(pixel_grid(&image), (1, 1));
        let wide = Image { width: 6, height: 4, rgba: vec![9; 6 * 4 * 4] };
        assert_eq!(pixel_grid(&wide), (3, 2));
    }

    #[test]
    fn rubbish_is_refused() {
        assert!(matches!(decode(b"not an image"), Err(Error::Image(_))));
    }
}
