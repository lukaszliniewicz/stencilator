//! Shrinking an image to one pixel per tile, exactly as Pillow does it.
//!
//! Pillow's box filter averages, with equal weights, the source pixels whose centres fall
//! inside each output pixel. It works in premultiplied alpha, in 22-bit fixed point, one
//! axis at a time with rounding to 8 bits between the passes. Matching a widely used
//! implementation to the bit makes the result easy to check; the steps follow Pillow 13's
//! `Resample.c`, `Convert.c` and `AlphaComposite.c`.

use crate::colour::Rgb;
use crate::source::Image;

const PRECISION_BITS: u32 = 32 - 8 - 2;

/// `(a * b + 127) / 255`, as Pillow computes it.
fn mul_div_255(a: u32, b: u32) -> u32 {
    let t = a * b + 128;
    ((t >> 8) + t) >> 8
}

fn shift_for_div_255(a: u32) -> u32 {
    ((a >> 8) + a) >> 8
}

/// Box-filter weights for one axis: for each output pixel, the first source pixel and the
/// fixed-point weights of the pixels from there on.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "C casts, reproduced on purpose; sizes are far below 2^24"
)]
fn coefficients(in_size: u32, out_size: u32) -> Vec<(usize, Vec<i32>)> {
    let scale = f64::from(in_size as f32) / f64::from(out_size);
    let filter_scale = scale.max(1.0);
    let support = 0.5 * filter_scale;
    let inverse = 1.0 / filter_scale;
    (0..out_size)
        .map(|xx| {
            let center = (f64::from(xx) + 0.5) * scale;
            let xmin = ((center - support + 0.5) as i64).max(0);
            let xmax = ((center + support + 0.5) as i64).min(i64::from(in_size));
            let weights: Vec<f64> = (xmin..xmax)
                .map(|x| {
                    let t = (x as f64 - center + 0.5) * inverse;
                    if t > -0.5 && t <= 0.5 { 1.0 } else { 0.0 }
                })
                .collect();
            let total: f64 = weights.iter().sum();
            let fixed = weights
                .iter()
                .map(|w| {
                    let w = if total == 0.0 { *w } else { w / total };
                    (0.5 + w * f64::from(1_u32 << PRECISION_BITS)) as i32
                })
                .collect();
            (xmin as usize, fixed)
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=255")]
fn clip8(sum: i32) -> u8 {
    (sum >> PRECISION_BITS).clamp(0, 255) as u8
}

/// One pass along rows (`horizontal`) or columns, on four-channel pixels.
fn pass(pixels: &[u8], width: usize, height: usize, out_size: u32, horizontal: bool) -> (Vec<u8>, usize, usize) {
    let in_size = if horizontal { width } else { height };
    #[allow(clippy::cast_possible_truncation, reason = "image sides fit u32")]
    let coeffs = coefficients(in_size as u32, out_size);
    let (out_w, out_h) = if horizontal { (out_size as usize, height) } else { (width, out_size as usize) };
    let mut out = vec![0_u8; out_w * out_h * 4];
    for y in 0..out_h {
        for x in 0..out_w {
            let (start, weights) = &coeffs[if horizontal { x } else { y }];
            let mut sums = [1_i32 << (PRECISION_BITS - 1); 4];
            for (i, &w) in weights.iter().enumerate() {
                let (sx, sy) = if horizontal { (start + i, y) } else { (x, start + i) };
                let p = (sy * width + sx) * 4;
                for c in 0..4 {
                    sums[c] += i32::from(pixels[p + c]) * w;
                }
            }
            let o = (y * out_w + x) * 4;
            for c in 0..4 {
                out[o + c] = clip8(sums[c]);
            }
        }
    }
    (out, out_w, out_h)
}

/// Resizes with Pillow's box filter, alpha premultiplied, rounding as Pillow does.
#[allow(clippy::cast_possible_truncation, reason = "results are 8-bit by construction")]
pub(crate) fn resize_box(image: &Image, width: u32, height: u32) -> Image {
    if (image.width, image.height) == (width, height) {
        return image.clone();
    }
    let mut pixels: Vec<u8> = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = u32::from(p[3]);
            [
                mul_div_255(p[0].into(), a) as u8,
                mul_div_255(p[1].into(), a) as u8,
                mul_div_255(p[2].into(), a) as u8,
                p[3],
            ]
        })
        .collect();
    let (mut w, mut h) = (image.width as usize, image.height as usize);
    let (in_w, in_h) = (i64::from(image.width), i64::from(image.height));
    let horizontal_first = !(in_h - i64::from(height) > 0 && in_h - i64::from(height) > (in_w - i64::from(width)) * 2);
    let order = if horizontal_first { [true, false] } else { [false, true] };
    for horizontal in order {
        let (needed, size) = if horizontal { (width != image.width, width) } else { (height != image.height, height) };
        if needed {
            (pixels, w, h) = pass(&pixels, w, h, size, horizontal);
        }
    }
    for p in pixels.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((255 * u32::from(*c)) / a).min(255) as u8;
            }
        }
    }
    Image { width, height, rgba: pixels }
}

/// Composites straight RGBA onto an opaque background colour, as Pillow's
/// `alpha_composite` does.
#[allow(clippy::cast_possible_truncation, reason = "results are 8-bit by construction")]
pub(crate) fn flatten(image: &Image, background: Rgb) -> Vec<Rgb> {
    const BITS: u32 = 7;
    image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let a = u32::from(p[3]);
            if a == 0 {
                return background;
            }
            // The background is opaque, so Pillow's general formula reduces to these weights.
            let outa255 = a * 255 + 255 * (255 - a);
            let coef1 = a * 255 * 255 * (1 << BITS) / outa255;
            let coef2 = 255 * (1 << BITS) - coef1;
            [0, 1, 2].map(|c| {
                let t = u32::from(p[c]) * coef1 + u32::from(background[c]) * coef2;
                (shift_for_div_255(t + (0x80 << BITS)) >> BITS) as u8
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
        Image { width, height, rgba }
    }

    #[test]
    fn uniform_blocks_keep_their_colour() {
        // 2 x 2 blocks of 3 x 3 pixels, shrunk to one pixel per block.
        let colours = [[200, 10, 30, 255], [0, 0, 0, 0], [5, 250, 5, 255], [90, 90, 200, 128]];
        let mut rgba = Vec::new();
        for y in 0..6 {
            for x in 0..6 {
                rgba.extend(colours[(y / 3) * 2 + x / 3]);
            }
        }
        let small = resize_box(&image(6, 6, rgba), 2, 2);
        assert_eq!(small.pixel(0, 0), [200, 10, 30, 255]);
        assert_eq!(small.pixel(1, 0)[3], 0);
        assert_eq!(small.pixel(0, 1), [5, 250, 5, 255]);
        assert_eq!(small.pixel(1, 1)[3], 128);
    }

    #[test]
    fn transparent_pixels_take_the_background() {
        let flat = flatten(&image(2, 1, vec![10, 20, 30, 255, 99, 99, 99, 0]), [200, 0, 0]);
        assert_eq!(flat, [[10, 20, 30], [200, 0, 0]]);
    }

    #[test]
    fn half_transparent_black_on_white_is_mid_grey() {
        assert_eq!(flatten(&image(1, 1, vec![0, 0, 0, 128]), [255, 255, 255]), [[127, 127, 127]]);
    }
}
