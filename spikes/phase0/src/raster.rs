//! Decoding PNG, JPEG and WebP, so the browser build carries the decoders
//! the engine will need and their size is measured, and resampling a decoded
//! image to the label map's resolution.

use image::imageops::{self, FilterType};
use image::{GenericImageView, RgbaImage};

use crate::render::Raster;

/// Width and height of an encoded image.
pub fn decoded_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    Ok(image.dimensions())
}

/// Decodes an image into premultiplied RGBA, the form the renderer produces.
pub fn decode(bytes: &[u8]) -> Result<Raster, String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?.into_rgba8();
    let (width, height) = image.dimensions();
    let mut rgba = image.into_raw();
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = u16::from(p[3]);
        for c in &mut p[..3] {
            *c = ((u16::from(*c) * a + 127) / 255) as u8;
        }
    }
    Ok(Raster { width, height, rgba })
}

/// Resamples a premultiplied raster to a new size. Filtering premultiplied
/// values keeps transparent pixels from darkening the edges.
pub fn resample(raster: &Raster, width: u32, height: u32) -> Result<Raster, String> {
    let image = RgbaImage::from_raw(raster.width, raster.height, raster.rgba.clone())
        .ok_or_else(|| "raster size does not match its pixels".to_owned())?;
    let resized = imageops::resize(&image, width.max(1), height.max(1), FilterType::Triangle);
    Ok(Raster { width: resized.width(), height: resized.height(), rgba: resized.into_raw() })
}
