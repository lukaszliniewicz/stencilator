//! Outlines of one paint's cells, as smooth shapes in micrometres.
//!
//! 1. The paint's cells, cropped to where it is, are blurred with a 3 by 3 box, which turns
//!    staircase edges into slopes.
//! 2. Marching squares at the half-way level finds the outline between cell centres.
//! 3. Douglas-Peucker drops points the outline does not need, within a tolerance.
//! 4. The polygon engine turns the rings into clean shapes with holes.
//!
//! At 0.1 mm cells, one blur and a 20 µm tolerance keep the outline within about 15 µm of
//! the true edge (Phase 0, spike 4).

use std::collections::BTreeMap;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::Overlay;
use i_overlay::core::overlay_rule::OverlayRule;

use crate::Error;
use crate::geometry::{Contour, Point, Shapes};

/// How outlines are traced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceSettings {
    /// Side of a cell in micrometres.
    pub cell_um: u32,
    /// Times the mask is blurred before tracing.
    pub blur_passes: u32,
    /// Largest distance, in micrometres, a simplified outline may stray from the traced one.
    pub tolerance_um: f64,
}

impl Default for TraceSettings {
    fn default() -> Self {
        TraceSettings { cell_um: 100, blur_passes: 1, tolerance_um: 20.0 }
    }
}

/// The outline of the `true` cells of a `width` by `height` mask, rows from the top. The
/// result has y pointing up from the mask's bottom edge and x from its left edge.
///
/// # Errors
///
/// [`Error::Geometry`] when the mask does not match its size or the cell size is zero.
pub fn trace_mask(mask: &[bool], width: u32, height: u32, settings: &TraceSettings) -> Result<Shapes, Error> {
    let (w, h) = (width as usize, height as usize);
    if mask.len() != w * h || settings.cell_um == 0 {
        return Err(Error::Geometry("The outline could not be traced: the cells do not match.".to_owned()));
    }
    let Some(crop) = Crop::of(mask, width, height) else {
        return Ok(Vec::new());
    };
    let mut field = crop.field(mask, width);
    for _ in 0..settings.blur_passes {
        field.blur();
    }
    let cell = f64::from(settings.cell_um);
    let height_um = f64::from(height) * cell;
    // Field node (x, y) is the centre of mask cell (crop.x0 + x - 1, crop.y0 + y - 1).
    let to_um =
        |(x, y): (f64, f64)| ((f64::from(crop.x0) + x - 0.5) * cell, height_um - (f64::from(crop.y0) + y - 0.5) * cell);
    let rings: Vec<Contour> = field
        .iso_lines()?
        .into_iter()
        .map(|ring| {
            let points: Vec<(f64, f64)> = ring.into_iter().map(to_um).collect();
            simplify_ring(&points, settings.tolerance_um)
                .into_iter()
                .map(|(x, y)| Point::new(micrometres(x), micrometres(y)))
                .collect()
        })
        .filter(|ring: &Contour| ring.len() >= 3)
        .collect();
    Ok(Overlay::from_subj_and_clip(&rings, &Vec::<Contour>::new()).overlay(OverlayRule::Subject, FillRule::EvenOdd))
}

#[allow(clippy::cast_possible_truncation, reason = "outlines lie within the mask, far inside i32 micrometres")]
fn micrometres(value: f64) -> i32 {
    value.round_ties_even() as i32
}

/// The rectangle of cells that holds every `true` one.
struct Crop {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl Crop {
    fn of(mask: &[bool], width: u32, height: u32) -> Option<Crop> {
        let mut crop: Option<Crop> = None;
        let w = width as usize;
        for y in 0..height {
            let row = &mask[y as usize * w..(y as usize + 1) * w];
            let (Some(first), Some(last)) = (row.iter().position(|&m| m), row.iter().rposition(|&m| m)) else {
                continue;
            };
            let (first, last) = (u32::try_from(first).ok()?, u32::try_from(last).ok()?);
            crop = Some(match crop {
                None => Crop { x0: first, y0: y, x1: last, y1: y },
                Some(c) => Crop { x0: c.x0.min(first), y0: c.y0, x1: c.x1.max(last), y1: y },
            });
        }
        crop
    }

    /// The cropped mask as a field of zeros and ones with an empty border one cell wide,
    /// which closes every outline.
    fn field(&self, mask: &[bool], width: u32) -> Field {
        let (w, h) = (self.x1 - self.x0 + 3, self.y1 - self.y0 + 3);
        let mut values = vec![0.0_f32; w as usize * h as usize];
        for y in self.y0..=self.y1 {
            for x in self.x0..=self.x1 {
                if mask[y as usize * width as usize + x as usize] {
                    values[(y - self.y0 + 1) as usize * w as usize + (x - self.x0 + 1) as usize] = 1.0;
                }
            }
        }
        Field { w, h, values }
    }
}

/// Samples on a grid of nodes, rows from the top.
struct Field {
    w: u32,
    h: u32,
    values: Vec<f32>,
}

/// A point where an outline crosses a grid edge, and the two crossings it joins.
struct Crossing {
    at: (f64, f64),
    links: [u64; 2],
    count: u8,
    seen: bool,
}

impl Field {
    fn at(&self, x: u32, y: u32) -> f32 {
        self.values[y as usize * self.w as usize + x as usize]
    }

    /// One pass of a 3 by 3 box blur, done as a row pass and a column pass, with zeros
    /// beyond the edge.
    fn blur(&mut self) {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut rows = vec![0.0_f32; self.values.len()];
        for y in 0..h {
            let row = &self.values[y * w..(y + 1) * w];
            for x in 0..w {
                let left = if x > 0 { row[x - 1] } else { 0.0 };
                let right = if x + 1 < w { row[x + 1] } else { 0.0 };
                rows[y * w + x] = left + row[x] + right;
            }
        }
        for y in 0..h {
            for x in 0..w {
                let up = if y > 0 { rows[(y - 1) * w + x] } else { 0.0 };
                let down = if y + 1 < h { rows[(y + 1) * w + x] } else { 0.0 };
                self.values[y * w + x] = (up + rows[y * w + x] + down) / 9.0;
            }
        }
    }

    /// Where the field crosses the edge from node (x, y) to the next node right of it or,
    /// when `down`, below it. The key orders crossings row by row.
    fn crossing(&self, x: u32, y: u32, down: bool) -> (u64, (f64, f64)) {
        let (x1, y1) = if down { (x, y + 1) } else { (x + 1, y) };
        let (v0, v1) = (self.at(x, y), self.at(x1, y1));
        let t = f64::from((ISO - v0) / (v1 - v0));
        let key = ((u64::from(y) * u64::from(self.w) + u64::from(x)) << 1) | u64::from(down);
        let (x, y) = (f64::from(x), f64::from(y));
        (key, (x + t * (f64::from(x1) - x), y + t * (f64::from(y1) - y)))
    }

    /// Closed iso-lines through the field at [`ISO`], in node coordinates.
    fn iso_lines(&self) -> Result<Vec<Vec<(f64, f64)>>, Error> {
        let mut crossings: BTreeMap<u64, Crossing> = BTreeMap::new();
        let mut join = |a: (u64, (f64, f64)), b: (u64, (f64, f64))| -> Result<(), Error> {
            for ((key, at), other) in [(a, b.0), (b, a.0)] {
                let crossing = crossings.entry(key).or_insert(Crossing { at, links: [0; 2], count: 0, seen: false });
                let slot = crossing.links.get_mut(usize::from(crossing.count)).ok_or_else(tangled)?;
                *slot = other;
                crossing.count += 1;
            }
            Ok(())
        };
        for y in 0..self.h - 1 {
            for x in 0..self.w - 1 {
                let v = [self.at(x, y), self.at(x + 1, y), self.at(x + 1, y + 1), self.at(x, y + 1)];
                let case = (0..4).filter(|&i| v[i] >= ISO).fold(0_u8, |c, i| c | (1 << i));
                let top = || self.crossing(x, y, false);
                let left = || self.crossing(x, y, true);
                let right = || self.crossing(x + 1, y, true);
                let bottom = || self.crossing(x, y + 1, false);
                // Saddles are decided by the average of the four corners.
                let centre_inside = (v[0] + v[1] + v[2] + v[3]) / 4.0 >= ISO;
                match case {
                    1 | 14 => join(left(), top())?,
                    2 | 13 => join(top(), right())?,
                    3 | 12 => join(left(), right())?,
                    4 | 11 => join(right(), bottom())?,
                    6 | 9 => join(top(), bottom())?,
                    7 | 8 => join(left(), bottom())?,
                    5 | 10 if centre_inside == (case == 5) => {
                        join(left(), bottom())?;
                        join(top(), right())?;
                    }
                    5 | 10 => {
                        join(left(), top())?;
                        join(right(), bottom())?;
                    }
                    _ => {}
                }
            }
        }
        let keys: Vec<u64> = crossings.keys().copied().collect();
        let mut rings = Vec::new();
        for start in keys {
            if crossings[&start].seen {
                continue;
            }
            let mut ring = Vec::new();
            let (mut previous, mut current) = (start, crossings[&start].links[0]);
            crossings.get_mut(&start).ok_or_else(tangled)?.seen = true;
            ring.push(crossings[&start].at);
            while current != start {
                let crossing = crossings.get_mut(&current).ok_or_else(tangled)?;
                if crossing.seen || crossing.count != 2 {
                    return Err(tangled());
                }
                crossing.seen = true;
                ring.push(crossing.at);
                let next = if crossing.links[0] == previous { crossing.links[1] } else { crossing.links[0] };
                previous = current;
                current = next;
            }
            rings.push(ring);
        }
        Ok(rings)
    }
}

/// The level the outline follows: half-way between outside (0) and inside (1).
const ISO: f32 = 0.5;

fn tangled() -> Error {
    Error::Geometry("The outline could not be traced. This is a fault in Stencilator; please report it.".to_owned())
}

/// Douglas-Peucker on a closed ring, anchored at its first point and at the point farthest
/// from it.
fn simplify_ring(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if points.len() < 4 {
        return points.to_vec();
    }
    let far = (1..points.len())
        .max_by(|&a, &b| distance2(points[0], points[a]).total_cmp(&distance2(points[0], points[b])))
        .unwrap_or(1);
    let mut closed = points.to_vec();
    closed.push(points[0]);
    let mut keep = vec![false; closed.len()];
    keep[0] = true;
    keep[far] = true;
    keep[closed.len() - 1] = true;
    // Spans still to simplify, kept on a stack rather than by recursion, so that long
    // outlines cannot overflow the browser's small stack.
    let mut spans = vec![(0, far), (far, closed.len() - 1)];
    while let Some((first, last)) = spans.pop() {
        if last <= first + 1 {
            continue;
        }
        let (a, b) = (closed[first], closed[last]);
        let (index, distance) = (first + 1..last)
            .map(|i| (i, segment_distance(closed[i], a, b)))
            .fold((first, -1.0), |best, candidate| if candidate.1 > best.1 { candidate } else { best });
        if distance > tolerance {
            keep[index] = true;
            spans.push((first, index));
            spans.push((index, last));
        }
    }
    closed.pop();
    closed.into_iter().zip(keep).filter(|&(_, k)| k).map(|(p, _)| p).collect()
}

fn distance2(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0) * (a.0 - b.0) + (a.1 - b.1) * (a.1 - b.1)
}

fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let length2 = distance2(a, b);
    if length2 == 0.0 {
        return distance2(p, a).sqrt();
    }
    let t = (((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / length2).clamp(0.0, 1.0);
    distance2(p, (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{area_mm2, bounds};

    /// A mask from rows of text: `#` is a cell of the paint.
    fn mask(rows: &[&str]) -> (Vec<bool>, u32, u32) {
        let cells = rows.iter().flat_map(|row| row.chars().map(|c| c == '#')).collect();
        (cells, u32::try_from(rows[0].len()).unwrap(), u32::try_from(rows.len()).unwrap())
    }

    fn trace(rows: &[&str], settings: &TraceSettings) -> Shapes {
        let (cells, w, h) = mask(rows);
        trace_mask(&cells, w, h, settings).unwrap()
    }

    /// A filled disc of the given radius, in cells, in a square mask.
    fn disc(radius: f64, side: u32) -> (Vec<bool>, u32) {
        let centre = f64::from(side) / 2.0;
        let cells = (0..side * side)
            .map(|i| {
                let (x, y) = (f64::from(i % side) + 0.5 - centre, f64::from(i / side) + 0.5 - centre);
                x * x + y * y <= radius * radius
            })
            .collect();
        (cells, side)
    }

    #[test]
    fn a_disc_comes_out_round_and_the_right_size() {
        let (cells, side) = disc(100.0, 220);
        let shapes = trace_mask(&cells, side, side, &TraceSettings::default()).unwrap();
        assert_eq!(shapes.len(), 1);
        // A radius of 100 cells of 0.1 mm: 10 mm.
        let area = area_mm2(&shapes);
        assert!((area - std::f64::consts::PI * 100.0).abs() < 1.5, "{area}");
        let (lo, hi) = bounds(&shapes).unwrap();
        assert!((hi.x - lo.x - 20_000).abs() < 100, "{lo:?} {hi:?}");
        // Simplified, but still a smooth circle rather than a staircase.
        assert!((40..400).contains(&shapes[0][0].len()), "{} points", shapes[0][0].len());
    }

    #[test]
    fn a_ring_has_a_hole_and_y_points_up() {
        let shapes = trace(
            &[
                "..........", //
                ".########.",
                ".########.",
                ".##....##.",
                ".##....##.",
                ".########.",
                ".########.",
                "..........",
                "..........",
                "..........",
            ],
            &TraceSettings { cell_um: 1000, blur_passes: 0, tolerance_um: 1.0 },
        );
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].len(), 2, "an outline and a hole");
        let (lo, hi) = bounds(&shapes).unwrap();
        // Unblurred, the outline runs half way between cells: x from 1 to 9 mm, and the ring
        // sits in the top rows of a 10 mm tall mask, so high up.
        assert_eq!((lo.x, hi.x), (1000, 9000));
        assert_eq!((lo.y, hi.y), (3000, 9000));
    }

    #[test]
    fn separate_blobs_are_separate_shapes() {
        let rows =
            ["###...###", "###...###", "###...###", ".........", ".........", "...###...", "...###...", "...###..."];
        assert_eq!(trace(&rows, &TraceSettings::default()).len(), 3);
        // A speck of two by two cells is gone after one blur: 0.2 mm is below what prints.
        assert_eq!(trace(&["....", ".##.", ".##.", "...."], &TraceSettings::default()), Shapes::new());
        // Cells touching at a corner, decided the same way every time.
        let diagonal = ["#.", ".#"];
        let settings = TraceSettings { cell_um: 1000, blur_passes: 0, tolerance_um: 1.0 };
        let once = trace(&diagonal, &settings);
        assert_eq!(once, trace(&diagonal, &settings));
        assert_ne!(once, Shapes::new());
    }

    #[test]
    fn nothing_traces_to_nothing_and_mismatches_are_refused() {
        assert_eq!(trace(&["....", "...."], &TraceSettings::default()), Shapes::new());
        assert!(trace_mask(&[true; 3], 2, 2, &TraceSettings::default()).is_err());
        let zero = TraceSettings { cell_um: 0, ..TraceSettings::default() };
        assert!(trace_mask(&[true; 4], 2, 2, &zero).is_err());
    }

    #[test]
    fn simplification_keeps_corners_and_drops_straight_runs() {
        let mut square = Vec::new();
        for i in 0..10 {
            square.push((f64::from(i), 0.0));
        }
        for i in 0..10 {
            square.push((10.0, f64::from(i)));
        }
        for i in 0..10 {
            square.push((10.0 - f64::from(i), 10.0));
        }
        for i in 0..10 {
            square.push((0.0, 10.0 - f64::from(i)));
        }
        let simple = simplify_ring(&square, 0.01);
        assert_eq!(simple, vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
    }
}
