//! Suggested supports across openings in a free-form plate.
//!
//! Sweep strips the width of a bridge across the plate in both directions. The
//! projection of a contour edge onto a strip marks where the full width is not
//! clear of that edge. A bridge spans one opening, with a tab entirely inside
//! solid material at each end. The shortest usable spans connect islands to the
//! rim, directly or through islands already attached.

use std::collections::BTreeSet;

use crate::geometry::{Point, Shapes, bounds, rectangle};

/// Keep the sweep bounded even for drawings far larger than a print bed.
const MAX_BANDS: i32 = 4096;
/// Try for a third bridge on pieces this long or longer.
const LARGE_ISLAND_UM: i32 = 20_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

impl Axis {
    /// In sweep coordinates, x runs along the bridge and y across it. The same
    /// operation puts a sweep point back in plate coordinates.
    fn orient(self, point: Point) -> Point {
        match self {
            Self::X => point,
            Self::Y => Point::new(point.y, point.x),
        }
    }
}

struct Edge {
    from: Point,
    to: Point,
    piece: usize,
}

struct Shadow {
    lo: i32,
    hi: i32,
    piece: usize,
    height: i32,
    /// Touches the strip at a tangent or along its edge rather than crossing it.
    grazes: bool,
    /// The one-based piece number if the edge crosses the strip's centre line, else 0.
    crossing: usize,
}

impl Edge {
    /// Project the section of this edge inside the strip, rounding outwards so
    /// the material left for an anchor cannot overhang the opening.
    fn shadow(&self, low: i32, high: i32, centre: i32) -> Shadow {
        let (a, b) = (self.from, self.to);
        let (enter, leave) = (low.max(a.y), high.min(b.y));
        let height = leave - enter;
        let grazes = height == 0 && (enter == low || leave == high);
        if a.y == b.y {
            return Shadow { lo: a.x.min(b.x), hi: a.x.max(b.x), piece: self.piece, height, grazes, crossing: 0 };
        }
        let rise = i64::from(b.y - a.y);
        let scaled_x = |y| i64::from(a.x) * rise + i64::from(b.x - a.x) * i64::from(y - a.y);
        let x0 = scaled_x(enter);
        let x1 = scaled_x(leave);
        // The projection stays between the edge's endpoints, in the plate's range.
        #[allow(clippy::cast_possible_truncation, reason = "projection stays in the plate's i32 coordinates")]
        let (lo, hi) = (x0.min(x1).div_euclid(rise) as i32, -(-x0.max(x1)).div_euclid(rise) as i32);
        let crossing = if (a.y..b.y).contains(&centre) { self.piece + 1 } else { 0 };
        Shadow { lo, hi, piece: self.piece, height, grazes, crossing }
    }
}

/// Space free of edges across the whole strip and inside a piece of the plate.
struct Landing {
    lo: i32,
    hi: i32,
    piece: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Bridge {
    axis: Axis,
    /// Opposite corners of the rectangle in plate coordinates.
    lo: Point,
    hi: Point,
    pub(crate) pieces: [usize; 2],
    pub(crate) gap: i32,
}

impl Bridge {
    fn overlaps(self, other: Self) -> bool {
        self.lo.x <= other.hi.x && other.lo.x <= self.hi.x && self.lo.y <= other.hi.y && other.lo.y <= self.hi.y
    }

    pub(crate) fn end(self, side: usize) -> Point {
        let (lo, hi) = (self.axis.orient(self.lo), self.axis.orient(self.hi));
        self.axis.orient(Point::new(if side == 0 { lo.x } else { hi.x }, i32::midpoint(lo.y, hi.y)))
    }

    pub(crate) fn shape(self) -> crate::geometry::Shape {
        vec![rectangle(self.lo.x, self.lo.y, self.hi.x, self.hi.y)]
    }

    /// Prefer a different side of the piece, with room between the tabs even
    /// when two spans set out from a corner.
    fn apart(self, side: usize, other: Self, other_side: usize, width: i32) -> bool {
        let (a, b) = (self.end(side), other.end(other_side));
        (self.axis != other.axis || side != other_side) && (a.x - b.x).abs().max((a.y - b.y).abs()) >= width
    }
}

/// Each line across the strip from one landing to the other must leave one
/// piece and enter the other. If the edges of each cover exactly one strip width
/// in y, there can be no extra crossings of either piece. Exclude contacts with
/// any other piece or just touching the edge of the strip.
fn crosses_one_opening(shadows: &[Shadow], pieces: [usize; 2], width: i32) -> bool {
    let mut coverage = [0_i64; 2];
    for shadow in shadows {
        if shadow.grazes {
            return false;
        }
        let Some(side) = pieces.iter().position(|&piece| piece == shadow.piece) else {
            return false;
        };
        coverage[side] += i64::from(shadow.height);
    }
    coverage == [i64::from(width); 2]
}

fn across_strip(shadows: &mut [Shadow], axis: Axis, low: i32, high: i32, tab: i32, out: &mut Vec<Bridge>) {
    shadows.sort_unstable_by_key(|shadow| shadow.lo);
    let mut shadow_end = 0;
    let mut piece = 0;
    let mut last: Option<(Landing, usize)> = None;
    for (i, shadow) in shadows.iter().enumerate() {
        if piece != 0 && shadow.lo - shadow_end > tab {
            let landing = Landing { lo: shadow_end, hi: shadow.lo, piece: piece - 1 };
            if let Some((left, start)) = last
                && left.piece != landing.piece
                && crosses_one_opening(&shadows[start..i], [left.piece, landing.piece], high - low)
            {
                out.push(Bridge {
                    axis,
                    lo: axis.orient(Point::new(left.hi - tab, low)),
                    hi: axis.orient(Point::new(landing.lo + tab, high)),
                    pieces: [left.piece, landing.piece],
                    gap: landing.lo - left.hi,
                });
            }
            last = Some((landing, i));
        }
        shadow_end = shadow_end.max(shadow.hi);
        // Each centre-line crossing toggles its own piece. Between shadows at most
        // one piece is present, so XOR of the toggled IDs gives its ID (or 0).
        piece ^= shadow.crossing;
    }
}

fn scan(
    solid: &Shapes,
    extents: &[(Point, Point)],
    axis: Axis,
    size: Point,
    width: i32,
    tab: i32,
    out: &mut Vec<Bridge>,
) {
    let size = axis.orient(size).y;
    let step = (width / 2).max((size + MAX_BANDS - 1) / MAX_BANDS);
    let centres = extents[1..].iter().map(|&(lo, hi)| i32::midpoint(axis.orient(lo).y, axis.orient(hi).y));
    // Also test the middle of each island, so a small island need not line up
    // with the regular bands to have a chance of getting a bridge.
    let mut bands: Vec<i32> = (0..=size - width)
        .step_by(usize::try_from(step).unwrap_or(1))
        .chain(centres.map(|centre| centre - width / 2).filter(|y| (0..=size - width).contains(y)))
        .collect();
    bands.sort_unstable();
    bands.dedup();

    let mut edges: Vec<Edge> = Vec::new();
    for (piece, shape) in solid.iter().enumerate() {
        for contour in shape {
            for (i, &point) in contour.iter().enumerate() {
                let (a, b) = (axis.orient(point), axis.orient(contour[(i + 1) % contour.len()]));
                let (from, to) = if a.y <= b.y { (a, b) } else { (b, a) };
                edges.push(Edge { from, to, piece });
            }
        }
    }
    edges.sort_unstable_by_key(|edge| edge.from.y);
    let mut remaining = edges.as_slice();
    let mut active: Vec<&Edge> = Vec::new();
    let mut shadows = Vec::new();
    for low in bands {
        let high = low + width;
        active.retain(|edge| edge.to.y >= low);
        let next = remaining.partition_point(|edge| edge.from.y <= high);
        let (arrived, later) = remaining.split_at(next);
        remaining = later;
        active.extend(arrived.iter().filter(|edge| edge.to.y >= low));
        shadows.clear();
        shadows.extend(active.iter().map(|edge| edge.shadow(low, high, low + width / 2)));
        across_strip(&mut shadows, axis, low, high, tab, out);
    }
}

/// Chosen bridges and the ends they put on each piece.
struct Selection {
    picked: Vec<usize>,
    /// Distance from the rim in the initial tree of bridges, or MAX if still loose.
    depth: Vec<usize>,
    /// Only ends leading towards the rim count as supports. Ends leading further
    /// into a nested group can't strengthen the group's connection to the rim.
    ends: Vec<Vec<(usize, usize)>>,
}

impl Selection {
    fn pick(&mut self, i: usize, side: usize, candidates: &[Bridge]) -> bool {
        let bridge = candidates[i];
        if self.picked.iter().any(|&j| bridge.overlaps(candidates[j])) {
            return false;
        }
        self.picked.push(i);
        self.ends[bridge.pieces[side]].push((i, side));
        true
    }
}

fn choose(candidates: &[Bridge], extents: &[(Point, Point)], width: i32) -> Selection {
    let mut neighbours = vec![Vec::new(); extents.len()];
    for (i, bridge) in candidates.iter().enumerate() {
        for piece in bridge.pieces {
            neighbours[piece].push(i);
        }
    }
    let mut selected =
        Selection { picked: Vec::new(), depth: vec![usize::MAX; extents.len()], ends: vec![Vec::new(); extents.len()] };
    selected.depth[0] = 0;
    let mut reachable: BTreeSet<usize> = neighbours[0].iter().copied().collect();
    while let Some(i) = reachable.pop_first() {
        let pieces = candidates[i].pieces;
        let Some(side) = pieces.iter().position(|&piece| selected.depth[piece] == usize::MAX) else {
            continue;
        };
        if selected.pick(i, side, candidates) {
            // The other end put this bridge on the frontier.
            let piece = pieces[side];
            selected.depth[piece] = selected.depth[pieces[1 - side]] + 1;
            reachable.extend(neighbours[piece].iter().copied());
        }
    }

    let wanted: Vec<usize> =
        extents.iter().map(|&(lo, hi)| if (hi.x - lo.x).max(hi.y - lo.y) >= LARGE_ISLAND_UM { 3 } else { 2 }).collect();
    for (i, &bridge) in candidates.iter().enumerate() {
        let [left, right] = bridge.pieces.map(|piece| selected.depth[piece]);
        let side = usize::from(right > left);
        let piece = bridge.pieces[side];
        let ends = &selected.ends[piece];
        if left != right
            && selected.depth[piece] != usize::MAX
            && ends.len() < wanted[piece]
            && ends.iter().all(|&(j, end)| bridge.apart(side, candidates[j], end, width))
        {
            selected.pick(i, side, candidates);
        }
    }
    selected
}

/// The suggested supports, and the pieces with one support towards the rim.
/// The first shape of `solid` is the rim.
pub(crate) fn suggestions(solid: &Shapes, size_um: (i32, i32), width: i32, tab: i32) -> (Vec<Bridge>, Vec<usize>) {
    if solid.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let extents: Vec<_> = solid
        .iter()
        .map(|shape| bounds(std::slice::from_ref(shape)).unwrap_or((Point::new(0, 0), Point::new(0, 0))))
        .collect();
    let mut candidates = Vec::new();
    let size = Point::new(size_um.0, size_um.1);
    scan(solid, &extents, Axis::X, size, width, tab, &mut candidates);
    scan(solid, &extents, Axis::Y, size, width, tab, &mut candidates);
    // For equal gaps, prefer the island's centre line to a bridge near a corner.
    candidates.sort_by_key(|bridge| {
        let middle = i32::midpoint(bridge.axis.orient(bridge.lo).y, bridge.axis.orient(bridge.hi).y);
        let off_centre: i32 = bridge
            .pieces
            .iter()
            .filter(|&&piece| piece != 0)
            .map(|&piece| {
                let (lo, hi) = extents[piece];
                (middle - i32::midpoint(bridge.axis.orient(lo).y, bridge.axis.orient(hi).y)).abs()
            })
            .sum();
        (bridge.gap, off_centre)
    });
    let selected = choose(&candidates, &extents, width);
    let chosen = selected.picked.iter().map(|&i| candidates[i]).collect();
    let single = selected
        .ends
        .iter()
        .enumerate()
        .skip(1)
        .filter_map(|(piece, ends)| (ends.len() == 1).then_some(piece))
        .collect();
    (chosen, single)
}
