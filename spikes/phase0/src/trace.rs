//! Outline tracing (spike 4): rendered artwork, to a label map, to smooth
//! polygons in micrometres.
//!
//! 1. Each pixel gets the nearest palette colour (transparent pixels get none).
//! 2. One label's mask is blurred with a 3 x 3 box, `blur_passes` times.
//! 3. Marching squares at 0.5 finds the outline between pixel centres.
//! 4. Douglas-Peucker simplification, then the overlay engine cleans the result.

use std::collections::BTreeMap;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::Overlay;
use i_overlay::core::overlay_rule::OverlayRule;

use crate::mesh::{Pt, Shapes};
use crate::render::Raster;

/// Which paint each pixel belongs to: 0 is none, `n` is palette entry `n - 1`.
#[derive(Debug, Clone)]
pub struct LabelMap {
    pub width: usize,
    pub height: usize,
    pub labels: Vec<u8>,
}

/// Opaque colours that cover at least `min_share` of the opaque pixels,
/// most common first. Anti-aliased edge colours fall below the threshold.
#[must_use]
pub fn palette(raster: &Raster, min_share: f64) -> Vec<[u8; 3]> {
    let mut counts: BTreeMap<[u8; 3], usize> = BTreeMap::new();
    let mut opaque = 0_usize;
    for p in raster.rgba.as_chunks::<4>().0 {
        if p[3] == 255 {
            *counts.entry([p[0], p[1], p[2]]).or_insert(0) += 1;
            opaque += 1;
        }
    }
    let mut common: Vec<([u8; 3], usize)> =
        counts.into_iter().filter(|&(_, n)| n as f64 >= min_share * opaque as f64).collect();
    common.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    common.into_iter().map(|(colour, _)| colour).collect()
}

/// Assigns every pixel to the nearest palette colour.
#[must_use]
pub fn classify(raster: &Raster, palette: &[[u8; 3]]) -> LabelMap {
    let labels = raster
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let alpha = u32::from(p[3]);
            if alpha < 128 {
                return 0;
            }
            let straight = [0, 1, 2].map(|c| (u32::from(p[c]) * 255 + alpha / 2) / alpha);
            let nearest = palette
                .iter()
                .enumerate()
                .min_by_key(|(_, colour)| {
                    (0..3).map(|c| (i64::from(straight[c]) - i64::from(colour[c])).pow(2)).sum::<i64>()
                })
                .map_or(0, |(i, _)| i + 1);
            nearest as u8
        })
        .collect();
    LabelMap { width: raster.width as usize, height: raster.height as usize, labels }
}

/// The outline of one label as cleaned shapes in micrometres, y up from the
/// bottom edge of a drawing `height_um` tall.
#[must_use]
pub fn trace(
    map: &LabelMap,
    label: u8,
    um_per_px: f64,
    height_um: f64,
    blur_passes: usize,
    tolerance_um: f64,
) -> Shapes {
    // A one-pixel empty border closes every outline.
    let (w, h) = (map.width + 2, map.height + 2);
    let mut field = vec![0.0_f32; w * h];
    for y in 0..map.height {
        for x in 0..map.width {
            if map.labels[y * map.width + x] == label {
                field[(y + 1) * w + x + 1] = 1.0;
            }
        }
    }
    for _ in 0..blur_passes {
        field = box_blur(&field, w, h);
    }
    let loops = marching_squares(&field, w, h, 0.5);
    let contours: Vec<Vec<Pt>> = loops
        .into_iter()
        .map(|points| {
            let mm: Vec<(f64, f64)> = points
                .iter()
                // Grid node (x, y) is the centre of pixel (x - 1, y - 1).
                .map(|&(x, y)| ((x - 0.5) * um_per_px, height_um - (y - 0.5) * um_per_px))
                .collect();
            simplify_closed(&mm, tolerance_um)
                .into_iter()
                .map(|(x, y)| Pt::new(x.round() as i32, y.round() as i32))
                .collect()
        })
        .filter(|contour: &Vec<Pt>| contour.len() >= 3)
        .collect();
    Overlay::from_subj_and_clip(&contours, &Vec::<Vec<Pt>>::new()).overlay(OverlayRule::Subject, FillRule::EvenOdd)
}

fn box_blur(field: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0_f32; field.len()];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    sum += field[yy * w + xx];
                }
            }
            out[y * w + x] = sum / 9.0;
        }
    }
    out
}

/// Closed iso-lines through a grid of samples, in grid coordinates.
fn marching_squares(field: &[f32], w: usize, h: usize, iso: f32) -> Vec<Vec<(f64, f64)>> {
    // A crossing on the grid edge leaving node (x, y) rightwards has id
    // 2 * (y * w + x); downwards, that plus one.
    let crossing = |id: usize| -> (f64, f64) {
        let node = id / 2;
        let (x, y) = (node % w, node / w);
        let (x1, y1) = if id.is_multiple_of(2) { (x + 1, y) } else { (x, y + 1) };
        let (v0, v1) = (field[y * w + x], field[y1 * w + x1]);
        let t = f64::from((iso - v0) / (v1 - v0));
        (x as f64 + t * (x1 as f64 - x as f64), y as f64 + t * (y1 as f64 - y as f64))
    };
    let mut neighbours: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut link = |a: usize, b: usize| {
        neighbours.entry(a).or_default().push(b);
        neighbours.entry(b).or_default().push(a);
    };
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let v = [field[y * w + x], field[y * w + x + 1], field[(y + 1) * w + x + 1], field[(y + 1) * w + x]];
            let inside = v.map(|s| s >= iso);
            let case = usize::from(inside[0])
                | usize::from(inside[1]) << 1
                | usize::from(inside[2]) << 2
                | usize::from(inside[3]) << 3;
            let top = 2 * (y * w + x);
            let left = top + 1;
            let right = 2 * (y * w + x + 1) + 1;
            let bottom = 2 * ((y + 1) * w + x);
            let centre_inside = (v[0] + v[1] + v[2] + v[3]) / 4.0 >= iso;
            match case {
                1 | 14 => link(left, top),
                2 | 13 => link(top, right),
                3 | 12 => link(left, right),
                4 | 11 => link(right, bottom),
                6 | 9 => link(top, bottom),
                7 | 8 => link(left, bottom),
                5 => {
                    if centre_inside {
                        link(left, bottom);
                        link(top, right);
                    } else {
                        link(left, top);
                        link(right, bottom);
                    }
                }
                10 => {
                    if centre_inside {
                        link(left, top);
                        link(right, bottom);
                    } else {
                        link(left, bottom);
                        link(top, right);
                    }
                }
                _ => {}
            }
        }
    }
    let mut loops = Vec::new();
    let mut visited: BTreeMap<usize, bool> = BTreeMap::new();
    for &start in neighbours.keys() {
        if visited.contains_key(&start) {
            continue;
        }
        let mut ring = vec![crossing(start)];
        visited.insert(start, true);
        let (mut previous, mut current) = (start, neighbours[&start][0]);
        while current != start {
            visited.insert(current, true);
            ring.push(crossing(current));
            let next = neighbours[&current].iter().copied().find(|&n| n != previous).unwrap_or(start);
            previous = current;
            current = next;
        }
        loops.push(ring);
    }
    loops
}

/// Douglas-Peucker on a closed ring, anchored at its first point and at the
/// point farthest from it.
fn simplify_closed(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if points.len() < 4 {
        return points.to_vec();
    }
    let far = (1..points.len())
        .max_by(|&a, &b| distance2(points[0], points[a]).total_cmp(&distance2(points[0], points[b])))
        .unwrap_or(1);
    let mut first: Vec<(f64, f64)> = points[..=far].to_vec();
    let mut second: Vec<(f64, f64)> = points[far..].to_vec();
    second.push(points[0]);
    first = douglas_peucker(&first, tolerance);
    second = douglas_peucker(&second, tolerance);
    first.pop();
    second.pop();
    first.extend(second);
    first
}

fn douglas_peucker(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let (index, distance) = points[1..points.len() - 1]
        .iter()
        .enumerate()
        .map(|(i, &p)| (i + 1, segment_distance(p, a, b)))
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .unwrap_or((0, 0.0));
    if distance <= tolerance {
        return vec![a, b];
    }
    let mut left = douglas_peucker(&points[..=index], tolerance);
    let right = douglas_peucker(&points[index..], tolerance);
    left.pop();
    left.extend(right);
    left
}

fn distance2(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let length2 = distance2(a, b);
    if length2 == 0.0 {
        return distance2(p, a).sqrt();
    }
    let t = (((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / length2).clamp(0.0, 1.0);
    distance2(p, (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))).sqrt()
}
