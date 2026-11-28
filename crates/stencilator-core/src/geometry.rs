//! Plane geometry for free-form plates, in integer micrometres with y pointing up.
//!
//! Integers are what make the native and browser builds agree to the bit: the polygon
//! engine (`i_overlay`) computes on them exactly, so no rounding can differ between them.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::Overlay;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::i_float::int::angle::Angle;
use i_overlay::i_float::int::point::IntPoint;
use i_overlay::mesh::int::arc::ArcOptions;
use i_overlay::mesh::int::outline::offset::IntOutlineOffset;
use i_overlay::mesh::int::style::{IntLineJoin, IntOutlineStyle};

use serde::Serialize;

use crate::Error;

/// Micrometres per millimetre.
pub const UM_PER_MM: f64 = 1000.0;

/// Whole micrometres from millimetres, for settings already checked to be in range.
#[allow(clippy::cast_possible_truncation, reason = "settings are checked to be far below 2^31 micrometres")]
pub(crate) fn um(mm: f64) -> i32 {
    (mm * UM_PER_MM).round_ties_even() as i32
}

pub(crate) fn mm(um: i32) -> f64 {
    f64::from(um) / UM_PER_MM
}

/// How one flat part fits in the rectangular usable area of a print bed.
///
/// As JSON this is `"as-drawn"`, `"quarter-turn"` or `"too-large"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BedFit {
    /// The part fits in the orientation of its export.
    AsDrawn,
    /// The part fits only when turned 90 degrees in the plane of the bed.
    QuarterTurn,
    /// Neither orientation fits.
    TooLarge,
}

impl BedFit {
    /// Whether either orientation fits.
    #[must_use]
    pub fn fits(self) -> bool {
        self != Self::TooLarge
    }
}

/// Compares two footprints in millimetres, preferring the exported orientation.
#[must_use]
pub fn bed_fit((w, h): (f64, f64), (bed_w, bed_h): (f64, f64)) -> BedFit {
    if w <= bed_w && h <= bed_h {
        BedFit::AsDrawn
    } else if h <= bed_w && w <= bed_h {
        BedFit::QuarterTurn
    } else {
        BedFit::TooLarge
    }
}

/// A point in micrometres.
pub type Point = IntPoint<i32>;
/// A closed ring of points, the last joined back to the first.
pub type Contour = Vec<Point>;
/// An outer contour, counter-clockwise, followed by its holes, clockwise.
pub type Shape = Vec<Contour>;
/// Separate shapes.
pub type Shapes = Vec<Shape>;

/// Rounded corners in steps of at most 1/64 turn: smooth at stencil sizes, and computed in
/// integers like everything else.
const ROUND_CORNERS: ArcOptions = ArcOptions { max_step: Angle::from_bits(1 << 26), rotation_precision: 5 };

/// Corners whose inside angle is below 1/32 turn (11.25 degrees) are cut rather than
/// extended to a point.
const SHARPEST_CORNER: Angle = Angle::from_bits(1 << 27);

/// A rectangle, counter-clockwise.
pub fn rectangle(x0: i32, y0: i32, x1: i32, y1: i32) -> Contour {
    vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)]
}

fn overlay(subject: &Shapes, clip: &Shapes, rule: OverlayRule) -> Shapes {
    if subject.is_empty() && clip.is_empty() {
        return Vec::new();
    }
    Overlay::from_subj_and_clip(subject, clip).overlay(rule, FillRule::NonZero)
}

/// The same area with overlaps merged, contours oriented and points on straight lines
/// dropped.
pub fn clean(shapes: &Shapes) -> Shapes {
    overlay(shapes, &Vec::new(), OverlayRule::Subject)
}

/// Everything in `a` or `b`.
pub fn union(a: &Shapes, b: &Shapes) -> Shapes {
    overlay(a, b, OverlayRule::Union)
}

/// What is in `a` but not in `b`.
pub fn difference(a: &Shapes, b: &Shapes) -> Shapes {
    overlay(a, b, OverlayRule::Difference)
}

/// What is in both `a` and `b`.
pub fn intersection(a: &Shapes, b: &Shapes) -> Shapes {
    overlay(a, b, OverlayRule::Intersect)
}

/// How [`offset`] turns outer corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corners {
    /// Arcs: every point of the result is exactly the distance away.
    Round,
    /// A straight cut across each corner: fewer points, slightly closer in.
    Bevel,
    /// The edges carried on until they meet, as the corner was; corners sharper than
    /// 1/32 turn are cut.
    Sharp,
}

/// Shapes grown by `distance_um`, or shrunk when it is negative. Shrinking can make thin
/// parts vanish or split, which is how narrow features are found.
///
/// # Errors
///
/// [`Error::Geometry`] when the result would leave the range of coordinates the polygon
/// engine can hold.
pub fn offset(shapes: &Shapes, distance_um: i32, corners: Corners) -> Result<Shapes, Error> {
    let cleaned = clean(shapes);
    if cleaned.is_empty() || distance_um == 0 {
        return Ok(cleaned);
    }
    let join = match corners {
        Corners::Round => IntLineJoin::Round(ROUND_CORNERS),
        Corners::Bevel => IntLineJoin::Bevel,
        Corners::Sharp => IntLineJoin::Miter(SHARPEST_CORNER),
    };
    let style = IntOutlineStyle::new(distance_um).line_join(join);
    cleaned.outline(&style).map_err(|_| Error::Geometry("The drawing is too large to work with.".to_owned()))
}

/// Twice the signed area of a contour in square micrometres: positive when it runs
/// counter-clockwise.
fn double_area(contour: &Contour) -> i128 {
    let n = contour.len();
    (0..n)
        .map(|i| {
            let (a, b) = (contour[i], contour[(i + 1) % n]);
            i128::from(a.x) * i128::from(b.y) - i128::from(b.x) * i128::from(a.y)
        })
        .sum()
}

/// The shapes moved by `dx`, `dy`.
pub fn translate(shapes: &[Shape], dx: i32, dy: i32) -> Shapes {
    shapes
        .iter()
        .map(|shape| shape.iter().map(|c| c.iter().map(|p| Point::new(p.x + dx, p.y + dy)).collect()).collect())
        .collect()
}

/// Area in square millimetres, holes subtracted.
#[allow(clippy::cast_precision_loss, reason = "a plate's area in square micrometres is far below 2^53")]
pub fn area_mm2(shapes: &[Shape]) -> f64 {
    let double: i128 = shapes.iter().flatten().map(double_area).sum();
    double as f64 / 2.0 / (UM_PER_MM * UM_PER_MM)
}

/// The smallest rectangle around every point, as `(min, max)`, or `None` for no points.
pub fn bounds(shapes: &[Shape]) -> Option<(Point, Point)> {
    let mut points = shapes.iter().flatten().flatten();
    let first = *points.next()?;
    Some(points.fold((first, first), |(lo, hi), p| {
        (Point::new(lo.x.min(p.x), lo.y.min(p.y)), Point::new(hi.x.max(p.x), hi.y.max(p.y)))
    }))
}

/// Whole micrometres written as millimetres, exactly and without trailing zeros.
pub(crate) struct Mm(pub(crate) i32);

impl std::fmt::Display for Mm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let um = self.0.unsigned_abs();
        let sign = if self.0 < 0 { "-" } else { "" };
        let (whole, part) = (um / 1000, um % 1000);
        if part == 0 {
            write!(f, "{sign}{whole}")
        } else {
            let digits = format!("{part:03}");
            write!(f, "{sign}{whole}.{}", digits.trim_end_matches('0'))
        }
    }
}

/// SVG path data for `shapes` in millimetres, with y turned to point down from `height_um`
/// as on a page. Holes are contours of their own, so fill it with the even-odd rule.
pub fn svg_path(shapes: &[Shape], height_um: i32) -> String {
    use std::fmt::Write as _;
    let mut d = String::new();
    for contour in shapes.iter().flatten() {
        for (i, p) in contour.iter().enumerate() {
            let command = if i == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{command}{} {}", Mm(p.x), Mm(height_um.saturating_sub(p.y)));
        }
        if !contour.is_empty() {
            d.push('Z');
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bed_fit_checks_both_flat_orientations_at_the_actual_size() {
        assert_eq!(bed_fit((103.5, 100.5), (104.0, 101.0)), BedFit::AsDrawn);
        assert_eq!(bed_fit((100.5, 103.5), (104.0, 101.0)), BedFit::QuarterTurn);
        assert_eq!(bed_fit((100.5, 103.5), (103.499, 101.0)), BedFit::TooLarge);
        assert_eq!(bed_fit((100.5, 103.5), (103.5, 103.5)), BedFit::AsDrawn);
    }

    fn square(x0: i32, y0: i32, side: i32) -> Shapes {
        vec![vec![rectangle(x0, y0, x0 + side, y0 + side)]]
    }

    #[test]
    fn paths_are_in_millimetres_from_the_top() {
        let framed = vec![vec![rectangle(0, 0, 10_000, 5_000), rectangle(1_500, 1_000, 2_250, 2_000)]];
        assert_eq!(svg_path(&framed, 5_000), "M0 5L10 5L10 0L0 0ZM1.5 4L2.25 4L2.25 3L1.5 3Z");
        assert_eq!(svg_path(&[], 5_000), "");
    }

    #[test]
    fn areas_subtract_holes() {
        let plate = difference(&square(0, 0, 10_000), &square(3000, 3000, 4000));
        assert_eq!(plate.len(), 1);
        assert_eq!(plate[0].len(), 2, "one outline and one hole");
        assert!((area_mm2(&plate) - 84.0).abs() < 1e-12);
        assert_eq!(bounds(&plate), Some((Point::new(0, 0), Point::new(10_000, 10_000))));
    }

    #[test]
    fn booleans_merge_and_cut() {
        let two = union(&square(0, 0, 2000), &square(1000, 1000, 2000));
        assert_eq!(two.len(), 1);
        assert!((area_mm2(&two) - 7.0).abs() < 1e-12);
        assert!((area_mm2(&intersection(&square(0, 0, 2000), &square(1000, 1000, 2000))) - 1.0).abs() < 1e-12);
        assert_eq!(difference(&square(0, 0, 1000), &square(0, 0, 1000)), Shapes::new());
        // Overlapping input comes out as one clean shape.
        let mut overlapping = square(0, 0, 2000);
        overlapping.extend(square(500, 500, 1000));
        assert_eq!(clean(&overlapping).len(), 1);
        assert!(bounds(&Vec::new()).is_none());
    }

    #[test]
    fn offsets_round_the_corners() {
        let grown = offset(&square(0, 0, 10_000), 1000, Corners::Round).unwrap();
        // A 12 mm square with its corners rounded to a 1 mm radius: 144 - (4 - pi).
        let expected = 144.0 - (4.0 - std::f64::consts::PI);
        assert!((area_mm2(&grown) - expected).abs() < 0.01, "{}", area_mm2(&grown));
        let shrunk = offset(&square(0, 0, 10_000), -1000, Corners::Round).unwrap();
        assert!((area_mm2(&shrunk) - 64.0).abs() < 1e-9);
        // Grown back with sharp corners, the square is whole again.
        let whole = offset(&shrunk, 1000, Corners::Sharp).unwrap();
        assert_eq!(whole[0][0].len(), 4);
        assert!((area_mm2(&whole) - 100.0).abs() < 1e-12);
        assert_eq!(bounds(&whole), Some((Point::new(0, 0), Point::new(10_000, 10_000))));
        assert_eq!(translate(&square(0, 0, 10), 5, -5), square(5, -5, 10));
    }

    #[test]
    fn shrinking_removes_thin_parts_and_splits_at_necks() {
        assert_eq!(offset(&square(0, 0, 400), -300, Corners::Round).unwrap(), Shapes::new());
        // Two 4 mm squares joined by a 0.4 mm neck fall apart when shrunk by 0.3 mm.
        let mut dumbbell = square(0, 0, 4000);
        dumbbell.extend(square(6000, 0, 4000));
        dumbbell.push(vec![rectangle(3000, 1800, 7000, 2200)]);
        assert_eq!(clean(&dumbbell).len(), 1);
        assert_eq!(offset(&dumbbell, -300, Corners::Round).unwrap().len(), 2);
    }
}
