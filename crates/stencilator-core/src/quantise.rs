//! Reducing tile colours to a small palette.
//!
//! Colour and greyscale modes cluster in CIELAB with weighted k-means (k-means++ seeding),
//! so the palette follows perceived differences rather than raw RGB distance. Black and
//! white mode splits lightness into two clusters, which is Otsu's threshold, and paints
//! them black and white.
//!
//! When the tiles have no more colours than requested, those colours are the palette,
//! unchanged. Clustering is seeded, so the same settings always give the same palette.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use crate::Error;
use crate::colour::{Lab, Rgb, lab_to_srgb, lightness, srgb_to_lab};
use crate::rng::Rng;

/// Distinct colours used to fit the palette; more add time, not quality.
const FIT_SAMPLE: usize = 20_000;
const MID_LIGHTNESS: f64 = 50.0;
const BLACK: Rgb = [0, 0, 0];
const WHITE: Rgb = [255, 255, 255];

/// How tile colours are reduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColourMode {
    /// Full colour.
    #[default]
    Colour,
    /// Shades of grey, clustered by lightness.
    Greyscale,
    /// Black and white, split by lightness.
    BlackWhite,
}

impl ColourMode {
    /// The mode's name in files and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            ColourMode::Colour => "colour",
            ColourMode::Greyscale => "greyscale",
            ColourMode::BlackWhite => "black_white",
        }
    }
}

impl fmt::Display for ColourMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for ColourMode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "colour" | "color" => Ok(ColourMode::Colour),
            "greyscale" | "grayscale" => Ok(ColourMode::Greyscale),
            "black_white" | "black-white" => Ok(ColourMode::BlackWhite),
            _ => Err(Error::Settings(format!("Unknown colour mode {s:?}; use colour, greyscale or black-white."))),
        }
    }
}

/// Distinct colours sorted from dark to light, and each input's index into them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// The colours, darkest first.
    pub colours: Vec<Rgb>,
    /// For each input colour, its index in `colours`.
    pub labels: Vec<u32>,
}

/// Reduces `rgb` to at most `n_colours` colours.
///
/// The result depends only on the input and the seed. It has fewer colours than requested
/// when the input has fewer distinct colours, and black and white mode ignores
/// `n_colours`.
///
/// # Errors
///
/// [`Error::Settings`] when there are no colours or no colours are requested.
pub fn quantise(rgb: &[Rgb], mode: ColourMode, n_colours: usize, seed: u64) -> Result<Palette, Error> {
    let fit = Fit::new(rgb, mode, n_colours, seed)?;
    let mut nearest: BTreeMap<Rgb, usize> = BTreeMap::new();
    for &c in rgb {
        nearest.entry(c).or_insert_with(|| fit.nearest(c));
    }
    Ok(canonical(&fit.colours, rgb.iter().map(|c| nearest[c])))
}

/// Paints fitted to a set of colours, which can then take any colour in: each paint is a
/// cluster centre in the mode's colour space, and a colour belongs to the nearest one.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fit {
    mode: ColourMode,
    /// Cluster centres, in CIELAB or, for the grey modes, lightness alone.
    centres: Vec<Lab>,
    /// The paint each centre stands for.
    pub(crate) colours: Vec<Rgb>,
}

impl Fit {
    /// Fits at most `n_colours` paints to `rgb`, as [`quantise`] does.
    pub(crate) fn new(rgb: &[Rgb], mode: ColourMode, n_colours: usize, seed: u64) -> Result<Fit, Error> {
        if rgb.is_empty() {
            return Err(Error::Settings("There are no colours to reduce.".to_owned()));
        }
        if n_colours < 1 {
            return Err(Error::Settings("At least one colour is required.".to_owned()));
        }
        let k = if mode == ColourMode::BlackWhite { 2 } else { n_colours };
        let mut rng = Rng::new(seed);

        // Cluster distinct values weighted by how often they occur: same result, less work.
        // Greyscale and black and white cluster lightness alone; a and b stay zero.
        let mut by_colour: BTreeMap<Rgb, u64> = BTreeMap::new();
        for &c in rgb {
            *by_colour.entry(c).or_default() += 1;
        }
        let mut distinct: Vec<(Lab, f64)> = Vec::new();
        {
            let mut keyed: Vec<(Lab, u64)> = by_colour.iter().map(|(&c, &n)| (features(mode, c), n)).collect();
            keyed.sort_by(|a, b| compare(&a.0, &b.0));
            for (lab, n) in keyed {
                #[allow(clippy::cast_precision_loss, reason = "tile counts are far below 2^52")]
                let n = n as f64;
                match distinct.last_mut() {
                    Some((last, count)) if compare(last, &lab).is_eq() => *count += n,
                    _ => distinct.push((lab, n)),
                }
            }
        }

        let centres: Vec<Lab> = if distinct.len() <= k {
            // Nothing to reduce: keep every colour exactly, however rare.
            distinct.iter().map(|d| d.0).collect()
        } else {
            let mut fit: Vec<(Lab, f64)> = distinct.clone();
            if mode != ColourMode::Colour {
                // Lightness only: snapping to 0.1 (far below a visible step) leaves at most
                // about a thousand values, so every tile is counted and no rare tone is lost.
                let mut snapped: BTreeMap<i64, f64> = BTreeMap::new();
                for (lab, n) in &fit {
                    #[allow(clippy::cast_possible_truncation, reason = "lightness is 0 to 100")]
                    let tenths = (lab[0] * 10.0).round_ties_even() as i64;
                    *snapped.entry(tenths).or_default() += n;
                }
                #[allow(clippy::cast_precision_loss, reason = "at most about a thousand values")]
                let snapped = snapped.into_iter().map(|(t, n)| ([t as f64 / 10.0, 0.0, 0.0], n));
                fit = snapped.collect();
            } else if fit.len() > FIT_SAMPLE {
                // Fit on a uniform sample of the distinct colours, each keeping its tile
                // count, so a rare colour is as likely to be seen as a common one.
                fit = rng.sample(fit.len(), FIT_SAMPLE).into_iter().map(|i| distinct[i]).collect();
            }
            let points: Vec<Lab> = fit.iter().map(|f| f.0).collect();
            let weights: Vec<f64> = fit.iter().map(|f| f.1).collect();
            kmeans(&points, &weights, k, &mut rng).0
        };

        let colours: Vec<Rgb> = match mode {
            ColourMode::Colour => centres.iter().map(|&c| lab_to_srgb(c)).collect(),
            ColourMode::Greyscale => centres.iter().map(|c| [lab_to_srgb([c[0], 0.0, 0.0])[1]; 3]).collect(),
            ColourMode::BlackWhite => {
                let lightest = centres.iter().map(|c| c[0]).fold(f64::NEG_INFINITY, f64::max);
                centres
                    .iter()
                    .map(|c| {
                        let light =
                            if centres.len() == 1 { c[0] >= MID_LIGHTNESS } else { c[0].total_cmp(&lightest).is_eq() };
                        if light { WHITE } else { BLACK }
                    })
                    .collect()
            }
        };
        Ok(Fit { mode, centres, colours })
    }

    /// Exactly these paints, nothing fitted.
    pub(crate) fn of(colours: &[Rgb], mode: ColourMode) -> Fit {
        Fit { mode, centres: colours.iter().map(|&c| features(mode, c)).collect(), colours: colours.to_vec() }
    }

    /// The paint a colour belongs to: the nearest centre, the first on a tie.
    pub(crate) fn nearest(&self, colour: Rgb) -> usize {
        nearest(&features(self.mode, colour), &self.centres)
    }

    /// Takes paint `index` away; its colours go to the nearest of the others.
    pub(crate) fn remove(&mut self, index: usize) {
        self.centres.remove(index);
        self.colours.remove(index);
    }

    /// Gives paint `index` another colour, and moves its centre there.
    pub(crate) fn set_colour(&mut self, index: usize, colour: Rgb) {
        self.colours[index] = colour;
        self.centres[index] = features(self.mode, colour);
    }

    /// How many paints there are.
    pub(crate) fn len(&self) -> usize {
        self.colours.len()
    }
}

/// Where a colour sits in the mode's colour space.
fn features(mode: ColourMode, colour: Rgb) -> Lab {
    let lab = srgb_to_lab(colour);
    if mode == ColourMode::Colour { lab } else { [lab[0], 0.0, 0.0] }
}

/// Paints sorted from dark to light, ties by RGB, with duplicates dropped, and where each of
/// the given colours went.
pub(crate) fn darkest_first(colours: &[Rgb]) -> (Vec<Rgb>, Vec<usize>) {
    let mut sorted = colours.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.sort_by(|a, b| lightness(*a).total_cmp(&lightness(*b)).then(a.cmp(b)));
    let rank: BTreeMap<Rgb, usize> = sorted.iter().enumerate().map(|(i, &c)| (c, i)).collect();
    let moved = colours.iter().map(|c| rank[c]).collect();
    (sorted, moved)
}

/// Lexicographic order on CIELAB values, as `numpy.unique` sorts rows.
fn compare(a: &Lab, b: &Lab) -> std::cmp::Ordering {
    a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])).then(a[2].total_cmp(&b[2]))
}

/// Drops unused and duplicate colours (centroids can round to the same RGB), then sorts
/// from dark to light, ties by RGB.
fn canonical(colours: &[Rgb], labels: impl Iterator<Item = usize> + Clone) -> Palette {
    let used: BTreeSet<usize> = labels.clone().collect();
    let mut distinct: Vec<Rgb> = used.iter().map(|&i| colours[i]).collect();
    distinct.sort_unstable();
    distinct.dedup();
    distinct.sort_by(|a, b| lightness(*a).total_cmp(&lightness(*b)).then(a.cmp(b)));
    let rank: BTreeMap<Rgb, u32> = distinct.iter().zip(0..).map(|(&c, i)| (c, i)).collect();
    Palette { labels: labels.map(|l| rank[&colours[l]]).collect(), colours: distinct }
}

fn squared_distance(a: &Lab, b: &Lab) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// Index of the nearest centroid; the first one on a tie.
fn nearest(point: &Lab, centroids: &[Lab]) -> usize {
    let mut best = (0, f64::INFINITY);
    for (i, c) in centroids.iter().enumerate() {
        let d = squared_distance(point, c);
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

/// Weighted Lloyd k-means with greedy k-means++ seeding.
///
/// `points` should be distinct. Iteration stops once no centroid moves more than 0.05 (in
/// CIELAB a just-noticeable difference is about 1), or after 100 rounds. Returns the
/// centroids and each point's label; there are fewer than `k` centroids when there are
/// fewer points, and a centroid may end up with no points.
pub(crate) fn kmeans(points: &[Lab], weights: &[f64], k: usize, rng: &mut Rng) -> (Vec<Lab>, Vec<usize>) {
    const MAX_ITER: usize = 100;
    const TOLERANCE: f64 = 0.05;
    if points.len() <= k {
        return (points.to_vec(), (0..points.len()).collect());
    }
    let mut centroids = seed(points, weights, k, rng);
    let mut labels: Vec<usize> = points.iter().map(|p| nearest(p, &centroids)).collect();
    for _ in 0..MAX_ITER {
        let mut mass = vec![0.0; centroids.len()];
        let mut sums = vec![[0.0; 3]; centroids.len()];
        for ((p, &w), &l) in points.iter().zip(weights).zip(&labels) {
            mass[l] += w;
            for d in 0..3 {
                sums[l][d] += p[d] * w;
            }
        }
        let mut shift: f64 = 0.0;
        for (i, centroid) in centroids.iter_mut().enumerate() {
            if mass[i] > 0.0 {
                let updated = sums[i].map(|s| s / mass[i]);
                for d in 0..3 {
                    shift = shift.max((updated[d] - centroid[d]).abs());
                }
                *centroid = updated;
            }
        }
        labels = points.iter().map(|p| nearest(p, &centroids)).collect();
        if shift <= TOLERANCE {
            break;
        }
    }
    (centroids, labels)
}

/// Greedy k-means++: of a few candidates per step, keep the one that lowers inertia most.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss, reason = "small k")]
fn seed(points: &[Lab], weights: &[f64], k: usize, rng: &mut Rng) -> Vec<Lab> {
    let trials = 2 + libm::log(k as f64) as usize;
    let first = points[rng.weighted(weights)];
    let mut chosen = vec![first];
    let mut closest: Vec<f64> = points.iter().map(|p| squared_distance(p, &first)).collect();
    for _ in 1..k {
        let potential: Vec<f64> = weights.iter().zip(&closest).map(|(w, d)| w * d).collect();
        if potential.iter().sum::<f64>() <= 0.0 {
            break; // every point already coincides with a centre
        }
        let mut best: Option<(f64, Lab, Vec<f64>)> = None;
        for _ in 0..trials {
            let candidate = points[rng.weighted(&potential)];
            let trial: Vec<f64> =
                points.iter().zip(&closest).map(|(p, &d)| d.min(squared_distance(p, &candidate))).collect();
            let inertia: f64 = trial.iter().zip(weights).map(|(d, w)| d * w).sum();
            if best.as_ref().is_none_or(|b| inertia < b.0) {
                best = Some((inertia, candidate, trial));
            }
        }
        if let Some((_, candidate, trial)) = best {
            chosen.push(candidate);
            closest = trial;
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(n: u8) -> Vec<Rgb> {
        (0..n).flat_map(|r| (0..n).map(move |g| [r * 4, g * 4, 128])).collect()
    }

    #[test]
    fn never_more_colours_than_requested() {
        let rgb = gradient(60);
        for k in [1, 2, 5, 16, 40] {
            let palette = quantise(&rgb, ColourMode::Colour, k, 0).unwrap();
            assert!(palette.colours.len() <= k, "{k}: {}", palette.colours.len());
            assert_eq!(palette.labels.len(), rgb.len());
            assert!(palette.labels.iter().all(|&l| (l as usize) < palette.colours.len()));
        }
    }

    #[test]
    fn same_seed_same_palette() {
        let rgb = gradient(50);
        assert_eq!(
            quantise(&rgb, ColourMode::Colour, 8, 4).unwrap(),
            quantise(&rgb, ColourMode::Colour, 8, 4).unwrap()
        );
    }

    #[test]
    fn palette_runs_dark_to_light() {
        let palette = quantise(&gradient(40), ColourMode::Colour, 10, 0).unwrap();
        let l: Vec<f64> = palette.colours.iter().map(|&c| lightness(c)).collect();
        assert!(l.windows(2).all(|w| w[0] <= w[1]), "{l:?}");
    }

    #[test]
    fn each_colour_goes_to_its_nearest_palette_entry() {
        let rgb = gradient(40);
        let palette = quantise(&rgb, ColourMode::Colour, 6, 0).unwrap();
        let centres: Vec<Lab> = palette.colours.iter().map(|&c| srgb_to_lab(c)).collect();
        // The labels follow the centroids, so allow for the rounding of centroids to RGB,
        // which moves a centre by about one CIELAB unit at most.
        let wrong = rgb
            .iter()
            .zip(&palette.labels)
            .filter(|(c, l)| {
                let lab = srgb_to_lab(**c);
                let own = squared_distance(&lab, &centres[**l as usize]).sqrt();
                centres.iter().any(|other| squared_distance(&lab, other).sqrt() + 2.0 < own)
            })
            .count();
        assert_eq!(wrong, 0);
    }

    #[test]
    fn black_and_white_gives_black_and_white() {
        let palette = quantise(&gradient(30), ColourMode::BlackWhite, 16, 0).unwrap();
        assert_eq!(palette.colours, [BLACK, WHITE]);
    }

    #[test]
    fn greyscale_gives_greys() {
        let palette = quantise(&gradient(30), ColourMode::Greyscale, 5, 0).unwrap();
        assert_eq!(palette.colours.len(), 5);
        assert!(palette.colours.iter().all(|c| c[0] == c[1] && c[1] == c[2]));
    }

    #[test]
    #[allow(clippy::cast_possible_truncation, reason = "each channel stays below 256")]
    fn large_inputs_are_fitted_on_a_sample() {
        // More distinct colours than the fitting sample takes.
        let rgb: Vec<Rgb> = (0..30_000_u32).map(|i| [(i % 7 * 30) as u8, (i % 200) as u8, (i / 200) as u8]).collect();
        let palette = quantise(&rgb, ColourMode::Colour, 8, 0).unwrap();
        assert_eq!(palette.colours.len(), 8);
        assert_eq!(palette, quantise(&rgb, ColourMode::Colour, 8, 0).unwrap());
    }

    #[test]
    fn bad_input_is_refused() {
        assert!(quantise(&[], ColourMode::Colour, 4, 0).is_err());
        assert!(quantise(&[[1, 2, 3]], ColourMode::Colour, 0, 0).is_err());
        assert!("purple".parse::<ColourMode>().is_err());
        assert_eq!("black-white".parse::<ColourMode>().unwrap(), ColourMode::BlackWhite);
    }
}
