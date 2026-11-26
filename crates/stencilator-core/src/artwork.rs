//! Free-form artwork: a picture laid on the canvas colour at a physical size, as a map of
//! square cells each holding one paint.
//!
//! 1. The palette is fitted on a copy of the picture at most [`FIT_SIDE`] pixels a side, as
//!    tile mode reduces colours: exactly when there are few, otherwise by clustering.
//! 2. Anti-aliasing leaves blends along the edges between flat paints, and clustering can
//!    make a paint of them. A paint that lies between two flat paints (mostly one exact
//!    colour each) and lives almost only on edges is such a blend; it is dropped, and its
//!    pixels go to the nearer of the two. Photos have no flat paints, so nothing is dropped
//!    from them, however speckled their paints are.
//! 3. In colour mode, a paint most of whose pixels are one exact colour, as in any flat
//!    artwork, takes that colour rather than the average of its pixels: the artwork's own
//!    colours come back, and the canvas colour stays exactly the canvas colour.
//! 4. The picture is resized to the cells with Pillow's bilinear filter, which keeps edges
//!    smooth when enlarging, laid on the canvas colour, and every cell takes the nearest
//!    paint.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use crate::Error;
use crate::chart::{CANVAS_MM, COLOURS};
use crate::colour::Rgb;
use crate::paints::merge_plan;
use crate::quantise::{ColourMode, Fit, darkest_first};
use crate::resample::{Filter, flatten, resize, resize_box};
use crate::source::Image;
use crate::text::thousands;

/// Accepted cell sizes in millimetres.
pub const CELL_MM: RangeInclusive<f64> = 0.05..=0.5;

/// Most cells a map may have: 400 by 400 mm at 0.1 mm, which keeps the browser's memory in
/// bounds.
pub const MAX_CELLS: u64 = 16_000_000;

/// Longest side of the copy the palette is fitted on; more pixels add time, not paints.
const FIT_SIDE: u32 = 512;

/// A blend lives on edges: at least this share of its pixels touch another paint.
const BLEND_EDGE_SHARE: f64 = 0.6;
/// A blend is small: at most this share of the picture.
const BLEND_AREA_SHARE: f64 = 0.25;
/// A blend lies this close (in RGB steps) to the line between two other paints...
const BLEND_RGB_DISTANCE: f64 = 20.0;
/// ...and properly between them, not at either end.
const BLEND_POSITION: RangeInclusive<f64> = 0.05..=0.95;
/// A paint takes an exact colour that makes up at least this share of its pixels.
const DOMINANT_SHARE: f64 = 0.5;

/// How large the artwork is made.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    /// This wide, in millimetres; the height follows.
    Width(f64),
    /// This tall, in millimetres; the width follows.
    Height(f64),
    /// As large as fits a box this wide and tall, in millimetres.
    Within(f64, f64),
}

/// Everything that decides a free-form label map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArtworkSettings {
    /// How large the artwork is made.
    pub size: Size,
    /// Side of a cell in millimetres: the finest detail the plates follow.
    pub cell_mm: f64,
    /// How colours are reduced.
    pub mode: ColourMode,
    /// Most paints.
    pub n_colours: usize,
    /// The colour of the canvas, which transparent parts of the picture take.
    pub background: Rgb,
    /// Seed for the colour clustering.
    pub seed: u64,
}

impl Default for ArtworkSettings {
    fn default() -> Self {
        ArtworkSettings {
            size: Size::Width(200.0),
            cell_mm: 0.1,
            mode: ColourMode::Colour,
            n_colours: 16,
            background: [255, 255, 255],
            seed: 0,
        }
    }
}

impl ArtworkSettings {
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
        match self.size {
            Size::Width(w) => mm("The width", w, &CANVAS_MM)?,
            Size::Height(h) => mm("The height", h, &CANVAS_MM)?,
            Size::Within(w, h) => {
                mm("The width", w, &CANVAS_MM)?;
                mm("The height", h, &CANVAS_MM)?;
            }
        }
        mm("The cell size", self.cell_mm, &CELL_MM)?;
        if !COLOURS.contains(&self.n_colours) {
            return Err(Error::Settings(format!(
                "The number of colours must be between {} and {}.",
                COLOURS.start(),
                COLOURS.end()
            )));
        }
        Ok(())
    }

    /// The cell side in micrometres.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "checked to be 50 to 500")]
    fn cell_um(&self) -> u32 {
        (self.cell_mm * 1000.0).round_ties_even() as u32
    }
}

/// Which paint each cell of the artwork holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelMap {
    /// Cells across.
    pub width: u32,
    /// Cells down.
    pub height: u32,
    /// Side of a cell in micrometres.
    pub cell_um: u32,
    /// The paints, darkest first.
    pub palette: Vec<Rgb>,
    /// Each cell's paint, rows from the top.
    pub labels: Vec<u8>,
}

impl LabelMap {
    /// Width in millimetres.
    pub fn width_mm(&self) -> f64 {
        f64::from(self.width) * f64::from(self.cell_um) / 1000.0
    }

    /// Height in millimetres.
    pub fn height_mm(&self) -> f64 {
        f64::from(self.height) * f64::from(self.cell_um) / 1000.0
    }

    /// Cells of each paint.
    pub fn cell_counts(&self) -> Vec<u64> {
        let mut counts = vec![0; self.palette.len()];
        for &l in &self.labels {
            counts[usize::from(l)] += 1;
        }
        counts
    }

    /// Which cells hold paint `index`.
    pub fn mask(&self, index: usize) -> Vec<bool> {
        self.labels.iter().map(|&l| usize::from(l) == index).collect()
    }

    /// The map with paints merged, as [`merge_plan`] describes.
    #[must_use]
    pub fn merged(&self, merges: &[(Rgb, Rgb)]) -> LabelMap {
        let (palette, moved) = merge_plan(&self.palette, merges);
        let labels = self.labels.iter().map(|&l| to_u8(moved[usize::from(l)])).collect();
        LabelMap { palette, labels, ..*self }
    }
}

/// How many cells a picture of `width` by `height` pixels makes, and their side in
/// micrometres.
///
/// # Errors
///
/// [`Error::Settings`] when a setting is out of range or there would be too many cells.
pub fn cell_grid(width: u32, height: u32, settings: &ArtworkSettings) -> Result<(u32, u32, u32), Error> {
    settings.check()?;
    if width == 0 || height == 0 {
        return Err(Error::Image("The image is empty.".to_owned()));
    }
    let (w, h) = (f64::from(width), f64::from(height));
    let (width_mm, height_mm) = match settings.size {
        Size::Width(mm) => (mm, mm * h / w),
        Size::Height(mm) => (mm * w / h, mm),
        Size::Within(bw, bh) => {
            let scale = (bw / w).min(bh / h);
            (w * scale, h * scale)
        }
    };
    let cell_um = settings.cell_um();
    let cells = |mm: f64| (mm * 1000.0 / f64::from(cell_um)).round_ties_even().max(1.0);
    let (x, y) = (cells(width_mm), cells(height_mm));
    let total = x * y;
    #[allow(clippy::cast_precision_loss, reason = "a small constant")]
    let too_many = total > MAX_CELLS as f64;
    if too_many {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "positive and below 10^12")]
        let total = total as u64;
        return Err(Error::Settings(format!(
            "At {} mm cells, a {width_mm:.0} by {height_mm:.0} mm picture makes {} cells; the limit is {}. \
             Choose larger cells or a smaller size.",
            settings.cell_mm,
            thousands(total),
            thousands(MAX_CELLS)
        )));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "between 1 and MAX_CELLS")]
    Ok((x as u32, y as u32, cell_um))
}

/// The picture resized to `width` by `height` cells with Pillow's bilinear filter and laid
/// on the canvas colour. Rows from the top.
pub fn sample_cells(image: &Image, width: u32, height: u32, background: Rgb) -> Vec<Rgb> {
    flatten(&resize(image, width, height, Filter::Bilinear), background)
}

/// The label map of a picture.
///
/// # Errors
///
/// As [`cell_grid`], and [`Error::Settings`] from the colour reduction.
pub fn label_picture(image: &Image, settings: &ArtworkSettings) -> Result<LabelMap, Error> {
    let (width, height, cell_um) = cell_grid(image.width, image.height, settings)?;

    let longest = image.width.max(image.height);
    let (fit_w, fit_h) = if longest > FIT_SIDE {
        let shrink =
            |side: u32| ((u64::from(side) * u64::from(FIT_SIDE) + u64::from(longest) / 2) / u64::from(longest)).max(1);
        (to_u32(shrink(image.width)), to_u32(shrink(image.height)))
    } else {
        (image.width, image.height)
    };
    let fit_rgb = flatten(&resize_box(image, fit_w, fit_h), settings.background);
    let mut fit = Fit::new(&fit_rgb, settings.mode, settings.n_colours, settings.seed)?;
    drop_blends(&mut fit, &fit_rgb, fit_w as usize);
    if settings.mode == ColourMode::Colour {
        take_exact_colours(&mut fit, &fit_rgb);
    }

    let cells = classify(&fit, &sample_cells(image, width, height, settings.background));
    // Paints no cell took are left out; paints that round to the same colour become one.
    let mut used = vec![false; fit.len()];
    for &c in &cells {
        used[usize::from(c)] = true;
    }
    let kept: Vec<usize> = (0..fit.len()).filter(|&i| used[i]).collect();
    let (palette, moved) = darkest_first(&kept.iter().map(|&i| fit.colours[i]).collect::<Vec<_>>());
    let mut rank = vec![0_u8; fit.len()];
    for (k, &i) in kept.iter().enumerate() {
        rank[i] = to_u8(moved[k]);
    }
    let labels = cells.iter().map(|&c| rank[usize::from(c)]).collect();
    Ok(LabelMap { width, height, cell_um, palette, labels })
}

fn to_u8(index: usize) -> u8 {
    u8::try_from(index).unwrap_or(u8::MAX)
}

fn to_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Each colour's paint. Pictures repeat colours in runs, so recent answers are remembered
/// in a small table indexed by a hash of the colour.
fn classify(fit: &Fit, rgb: &[Rgb]) -> Vec<u8> {
    const SLOTS_BITS: u32 = 16;
    let mut remembered: Vec<(u32, u8)> = vec![(u32::MAX, 0); 1 << SLOTS_BITS];
    rgb.iter()
        .map(|&c| {
            let key = u32::from_be_bytes([0, c[0], c[1], c[2]]);
            let slot = (key.wrapping_mul(2_654_435_769) >> (32 - SLOTS_BITS)) as usize;
            if remembered[slot].0 == key {
                remembered[slot].1
            } else {
                let paint = to_u8(fit.nearest(c));
                remembered[slot] = (key, paint);
                paint
            }
        })
        .collect()
}

/// Drops paints no pixel takes, then, one at a time and smallest first, paints that are
/// blends along the edges between two flat ones.
fn drop_blends(fit: &mut Fit, rgb: &[Rgb], width: usize) {
    let height = rgb.len() / width.max(1);
    #[allow(clippy::cast_precision_loss, reason = "pixel counts are far below 2^52")]
    let total = rgb.len() as f64;
    while fit.len() > 1 {
        let labels = classify(fit, rgb);
        let mut count = vec![0_u64; fit.len()];
        let mut on_edge = vec![0_u64; fit.len()];
        for y in 0..height {
            for x in 0..width {
                let here = labels[y * width + x];
                count[usize::from(here)] += 1;
                let differs = |nx: usize, ny: usize| labels[ny * width + nx] != here;
                if (x > 0 && differs(x - 1, y))
                    || (x + 1 < width && differs(x + 1, y))
                    || (y > 0 && differs(x, y - 1))
                    || (y + 1 < height && differs(x, y + 1))
                {
                    on_edge[usize::from(here)] += 1;
                }
            }
        }
        if let Some(unused) = count.iter().position(|&n| n == 0) {
            fit.remove(unused);
            continue;
        }
        let flat = flat_paints(rgb, &labels, &count);
        #[allow(clippy::cast_precision_loss, reason = "pixel counts are far below 2^52")]
        let share = |n: u64| n as f64 / total;
        #[allow(clippy::cast_precision_loss, reason = "pixel counts are far below 2^52")]
        let edge_share = |i: usize| on_edge[i] as f64 / count[i] as f64;
        let blend = (0..fit.len())
            .filter(|&i| edge_share(i) >= BLEND_EDGE_SHARE && share(count[i]) <= BLEND_AREA_SHARE)
            .filter(|&i| {
                let ends: Vec<Rgb> = (0..fit.len()).filter(|&j| j != i && flat[j]).map(|j| fit.colours[j]).collect();
                ends.iter().enumerate().any(|(a, &ca)| ends[a + 1..].iter().any(|&cb| between(fit.colours[i], ca, cb)))
            })
            .min_by_key(|&i| (count[i], i));
        match blend {
            Some(i) => fit.remove(i),
            None => return,
        }
    }
}

/// Which paints are flat: at least [`DOMINANT_SHARE`] of their pixels are one exact colour.
fn flat_paints(rgb: &[Rgb], labels: &[u8], count: &[u64]) -> Vec<bool> {
    // Paint and colour in one sortable key; equal keys then sit together.
    let mut keys: Vec<u32> = rgb.iter().zip(labels).map(|(c, &l)| u32::from_be_bytes([l, c[0], c[1], c[2]])).collect();
    keys.sort_unstable();
    let mut longest = vec![0_u64; count.len()];
    for run in keys.chunk_by(|a, b| a == b) {
        let paint = usize::from(run[0].to_be_bytes()[0]);
        longest[paint] = longest[paint].max(run.len() as u64);
    }
    #[allow(clippy::cast_precision_loss, reason = "pixel counts are far below 2^52")]
    let flat = longest.iter().zip(count).map(|(&run, &n)| run as f64 >= DOMINANT_SHARE * n as f64).collect();
    flat
}

/// Gives each paint the exact colour that most of its pixels have, if one does.
fn take_exact_colours(fit: &mut Fit, rgb: &[Rgb]) {
    let labels = classify(fit, rgb);
    let mut colours: Vec<BTreeMap<Rgb, u64>> = vec![BTreeMap::new(); fit.len()];
    for (&c, &l) in rgb.iter().zip(&labels) {
        *colours[usize::from(l)].entry(c).or_default() += 1;
    }
    for (i, histogram) in colours.iter().enumerate() {
        let total: u64 = histogram.values().sum();
        let Some((&colour, &n)) = histogram.iter().max_by_key(|&(c, n)| (*n, std::cmp::Reverse(*c))) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss, reason = "pixel counts are far below 2^52")]
        let dominant = n as f64 >= DOMINANT_SHARE * total as f64;
        if dominant {
            fit.set_colour(i, colour);
        }
    }
}

/// Whether `c` lies on the RGB line from `a` to `b`, away from both ends, as a mix of the
/// two does.
fn between(colour: Rgb, start: Rgb, end: Rgb) -> bool {
    let v = |p: Rgb| p.map(f64::from);
    let (colour, start, end) = (v(colour), v(start), v(end));
    let dir = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let length2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
    if length2 == 0.0 {
        return false;
    }
    let to = [colour[0] - start[0], colour[1] - start[1], colour[2] - start[2]];
    let along = (to[0] * dir[0] + to[1] * dir[1] + to[2] * dir[2]) / length2;
    let off = [along * dir[0] - to[0], along * dir[1] - to[1], along * dir[2] - to[2]];
    BLEND_POSITION.contains(&along)
        && (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt() <= BLEND_RGB_DISTANCE
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = [200, 30, 40];
    const BLUE: Rgb = [30, 60, 190];
    const WHITE: Rgb = [255, 255, 255];

    /// A `side` by `side` picture drawn at four times the size and averaged down, as
    /// anti-aliasing does (in premultiplied alpha): a red disc and a blue square on `ground`.
    fn drawing(side: u32, ground: [u8; 4]) -> Image {
        let paint = |x: f64, y: f64| -> [u8; 4] {
            let s = f64::from(side);
            let (dx, dy, r) = (x - 0.35 * s, y - 0.5 * s, 0.25 * s);
            if dx * dx + dy * dy < r * r {
                [RED[0], RED[1], RED[2], 255]
            } else if (0.65 * s..0.9 * s).contains(&x) && (0.3 * s..0.7 * s).contains(&y) {
                [BLUE[0], BLUE[1], BLUE[2], 255]
            } else {
                ground
            }
        };
        let mut rgba = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let mut sum = [0_u32; 4];
                for sy in 0..4 {
                    for sx in 0..4 {
                        let p = paint(f64::from(x * 4 + sx) / 4.0 + 0.125, f64::from(y * 4 + sy) / 4.0 + 0.125);
                        for c in 0..3 {
                            sum[c] += u32::from(p[c]) * u32::from(p[3]);
                        }
                        sum[3] += u32::from(p[3]);
                    }
                }
                let alpha = sum[3];
                let straight = |s: u32| (s + alpha / 2).checked_div(alpha).map_or(0, |v| u8::try_from(v).unwrap());
                rgba.extend([
                    straight(sum[0]),
                    straight(sum[1]),
                    straight(sum[2]),
                    u8::try_from((alpha + 8) / 16).unwrap(),
                ]);
            }
        }
        Image { width: side, height: side, rgba }
    }

    #[test]
    fn anti_aliased_edges_do_not_become_paints() {
        let image = drawing(120, [255, 255, 255, 255]);
        let settings = ArtworkSettings { size: Size::Width(30.0), cell_mm: 0.1, ..ArtworkSettings::default() };
        let map = label_picture(&image, &settings).unwrap();
        assert_eq!((map.width, map.height, map.cell_um), (300, 300, 100));
        // Exactly the colours drawn, darkest first.
        assert_eq!(map.palette, [BLUE, RED, WHITE]);
        // The disc covers pi * 0.25^2 of the picture, the square 0.25 * 0.4.
        let counts = map.cell_counts();
        let area = |n: u64| f64::from(u32::try_from(n).unwrap()) / 90_000.0;
        assert!((area(counts[1]) - std::f64::consts::PI * 0.0625).abs() < 0.01, "{counts:?}");
        assert!((area(counts[0]) - 0.1).abs() < 0.01, "{counts:?}");
    }

    #[test]
    fn transparency_takes_the_canvas_colour() {
        let image = drawing(80, [0, 0, 0, 0]);
        let canvas = [240, 230, 200];
        let settings = ArtworkSettings { size: Size::Width(20.0), background: canvas, ..ArtworkSettings::default() };
        let map = label_picture(&image, &settings).unwrap();
        assert_eq!(map.palette, [BLUE, RED, canvas]);
    }

    #[test]
    fn pixel_art_keeps_every_paint() {
        // The fox: four paints, the dark red one lying between near-black and orange, but in
        // blocks, not along edges.
        let bytes = include_bytes!("../../../fixtures/pixel-art/fox.png");
        let image = crate::source::decode(bytes).unwrap();
        let settings = ArtworkSettings { size: Size::Width(110.0), cell_mm: 0.25, ..ArtworkSettings::default() };
        let map = label_picture(&image, &settings).unwrap();
        assert_eq!(map.palette, [[0x2b, 0x23, 0x20], [0x9c, 0x3d, 0x17], [0xe8, 0x77, 0x2e], [255, 255, 255]]);
        assert_eq!((map.width, map.height), (440, 440));
        // Each fox pixel is 5 mm: 20 by 20 cells.
        assert_eq!(map.cell_counts(), [16 * 400, 50 * 400, 160 * 400, 258 * 400]);
        let merged = map.merged(&[([0x9c, 0x3d, 0x17], [0xe8, 0x77, 0x2e])]);
        assert_eq!(merged.cell_counts(), [16 * 400, 210 * 400, 258 * 400]);
        assert_eq!(merged.mask(0).iter().filter(|&&m| m).count(), 16 * 400);
    }

    #[test]
    fn grey_modes_and_the_same_result_every_time() {
        let image = drawing(60, [255, 255, 255, 255]);
        let grey =
            ArtworkSettings { size: Size::Height(15.0), mode: ColourMode::Greyscale, ..ArtworkSettings::default() };
        let map = label_picture(&image, &grey).unwrap();
        assert!(map.palette.iter().all(|c| c[0] == c[1] && c[1] == c[2]), "{:?}", map.palette);
        assert_eq!(map, label_picture(&image, &grey).unwrap());
        let two = ArtworkSettings { mode: ColourMode::BlackWhite, ..grey };
        assert_eq!(label_picture(&image, &two).unwrap().palette, [[0, 0, 0], [255, 255, 255]]);
    }

    #[test]
    fn sizes_follow_the_picture_and_the_limits_hold() {
        let grid = |size| cell_grid(400, 200, &ArtworkSettings { size, ..ArtworkSettings::default() });
        assert_eq!(grid(Size::Width(100.0)).unwrap(), (1000, 500, 100));
        assert_eq!(grid(Size::Height(100.0)).unwrap(), (2000, 1000, 100));
        assert_eq!(grid(Size::Within(100.0, 100.0)).unwrap(), (1000, 500, 100));
        assert_eq!(grid(Size::Within(300.0, 50.0)).unwrap(), (1000, 500, 100));
        let message = grid(Size::Width(1000.0)).unwrap_err().to_string();
        assert_eq!(
            message,
            "At 0.1 mm cells, a 1000 by 500 mm picture makes 50,000,000 cells; the limit is 16,000,000. \
             Choose larger cells or a smaller size."
        );
        let fine = ArtworkSettings { cell_mm: 0.01, ..ArtworkSettings::default() };
        assert_eq!(cell_grid(10, 10, &fine).unwrap_err().to_string(), "The cell size must be between 0.05 and 0.5 mm.");
        assert!(grid(Size::Width(0.0)).is_err());
    }

    #[test]
    fn blends_lie_between_two_paints() {
        assert!(between([128, 128, 128], [0, 0, 0], [255, 255, 255]));
        assert!(!between([250, 250, 250], [0, 0, 0], [255, 255, 255]), "at an end");
        assert!(!between([200, 0, 0], [0, 0, 0], [255, 255, 255]), "off the line");
    }

    #[test]
    fn a_speckled_photo_keeps_its_paints() {
        // A noisy gradient along one line of colours: every paint lies between two others
        // and is speckled, so mostly on edges, yet none is a blend, since none is flat.
        let mut state = 12_345_u64;
        let mut rgba = Vec::new();
        for _ in 0..120_u32 {
            for x in 0..160_u32 {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                let noise = i32::try_from(state >> 59).unwrap() - 16;
                let ramp = |top: u32| i32::try_from(x * top / 160).unwrap() + noise;
                rgba.extend([ramp(255), ramp(200), ramp(100)].map(|v| u8::try_from(v.clamp(0, 255)).unwrap()));
                rgba.push(255);
            }
        }
        let photo = Image { width: 160, height: 120, rgba };
        let map = label_picture(&photo, &ArtworkSettings { size: Size::Width(40.0), ..ArtworkSettings::default() });
        assert_eq!(map.unwrap().palette.len(), 16);
    }
}
