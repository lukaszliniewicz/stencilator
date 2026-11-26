//! sRGB to CIELAB (D65) and back, so that colours are clustered by how different they look.
//!
//! Powers and cube roots come from `libm`, which gives the same bits natively and in the
//! browser. The matrices are written out to the last bit (the inverse as NumPy computes
//! it), so conversions agree exactly with the reference results.

/// An 8-bit sRGB colour.
pub type Rgb = [u8; 3];

/// A CIELAB colour: lightness 0 to 100, then a and b.
pub type Lab = [f64; 3];

const RGB_TO_XYZ: [[f64; 3]; 3] = [
    [0.412_456_4, 0.357_576_1, 0.180_437_5],
    [0.212_672_9, 0.715_152_2, 0.072_175_0],
    [0.019_333_9, 0.119_192_0, 0.950_304_1],
];

/// NumPy's inverse of `RGB_TO_XYZ`, bit for bit.
const XYZ_TO_RGB: [[f64; 3]; 3] = [
    [
        f64::from_bits(0x4009_ec73_95c7_0415),
        f64::from_bits(0xbff8_981e_e829_a02e),
        f64::from_bits(0xbfdf_e7f0_dc74_c340),
    ],
    [
        f64::from_bits(0xbfef_043a_f2b4_17e9),
        f64::from_bits(0x3ffe_0424_0925_12c5),
        f64::from_bits(0x3fa5_46d6_bd41_f814),
    ],
    [
        f64::from_bits(0x3fac_7d4b_56e0_1a9b),
        f64::from_bits(0xbfca_1d84_e9cf_2205),
        f64::from_bits(0x3ff0_ea64_ee94_3dd1),
    ],
];

/// D65 reference white for the sRGB primaries: the rows of `RGB_TO_XYZ` summed.
const WHITE: [f64; 3] = [
    RGB_TO_XYZ[0][0] + RGB_TO_XYZ[0][1] + RGB_TO_XYZ[0][2],
    RGB_TO_XYZ[1][0] + RGB_TO_XYZ[1][1] + RGB_TO_XYZ[1][2],
    RGB_TO_XYZ[2][0] + RGB_TO_XYZ[2][1] + RGB_TO_XYZ[2][2],
];

const EPSILON: f64 = 216.0 / 24389.0;
const KAPPA: f64 = 24389.0 / 27.0;
/// Encoded value where the sRGB curve turns linear.
const SRGB_KNEE: f64 = 0.04045;
/// The same point in linear light.
const LINEAR_KNEE: f64 = 0.003_130_8;

fn multiply(matrix: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| v[0] * row[0] + v[1] * row[1] + v[2] * row[2])
}

/// Converts an 8-bit sRGB colour to CIELAB.
pub fn srgb_to_lab(rgb: Rgb) -> Lab {
    let linear = rgb.map(|v| {
        let c = f64::from(v) / 255.0;
        if c <= SRGB_KNEE { c / 12.92 } else { libm::pow((c + 0.055) / 1.055, 2.4) }
    });
    let xyz = multiply(&RGB_TO_XYZ, linear);
    let f = [0, 1, 2].map(|i| {
        let t = xyz[i] / WHITE[i];
        if t > EPSILON { libm::cbrt(t) } else { (KAPPA * t + 16.0) / 116.0 }
    });
    [116.0 * f[1] - 16.0, 500.0 * (f[0] - f[1]), 200.0 * (f[1] - f[2])]
}

/// Converts a CIELAB colour to 8-bit sRGB, clipping colours outside the sRGB gamut.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=255 first")]
pub fn lab_to_srgb(lab: Lab) -> Rgb {
    let fy = (lab[0] + 16.0) / 116.0;
    let f = [fy + lab[1] / 500.0, fy, fy - lab[2] / 200.0];
    let xyz = [0, 1, 2].map(|i| {
        let cubed = libm::pow(f[i], 3.0);
        (if cubed > EPSILON { cubed } else { (116.0 * f[i] - 16.0) / KAPPA }) * WHITE[i]
    });
    multiply(&XYZ_TO_RGB, xyz).map(|linear| {
        let linear = linear.clamp(0.0, 1.0);
        let c = if linear <= LINEAR_KNEE { 12.92 * linear } else { 1.055 * libm::pow(linear, 1.0 / 2.4) - 0.055 };
        (c * 255.0).round_ties_even().clamp(0.0, 255.0) as u8
    })
}

/// CIELAB lightness of an sRGB colour, 0 (black) to 100 (white).
pub fn lightness(rgb: Rgb) -> f64 {
    srgb_to_lab(rgb)[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_point_is_exact() {
        assert_eq!(WHITE.map(|v| format!("{v:?}")), ["0.95047", "1.0000001", "1.08883"]);
    }

    #[test]
    fn every_grey_survives_the_round_trip() {
        for v in 0..=255 {
            assert_eq!(lab_to_srgb(srgb_to_lab([v, v, v])), [v, v, v]);
        }
    }
}
