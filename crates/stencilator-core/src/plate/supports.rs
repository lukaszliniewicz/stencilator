//! Flat and raised supports, with manual edits replayed against the current plate.

use std::collections::{BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::bridges::suggestions;
use crate::colour::Rgb;
use crate::geometry::{Corners, Point, Shape, Shapes, bounds, difference, intersection, mm, offset, svg_path, um};
use crate::mesh::{ALIGN_UM, Slab};
use crate::project::Colour;

/// Maximum number of manually edited supports in a project.
pub const MAX_SUPPORT_EDITS: usize = 512;
const STEP: i32 = 200;
const SHORT_SPAN: i32 = 30_000;
const MAX_REACH: i32 = 8_000;

/// Settings for raised connectors. All dimensions are trial settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RaisedSettings {
    /// Width of the beam and square landings.
    pub width_mm: f64,
    /// Minimum clearance below the beam, from the work face at z = 0.
    pub clearance_mm: f64,
    /// Vertical thickness of the beam, above the clearance and any steps.
    pub beam_mm: f64,
    /// Report free spans longer than this.
    pub warn_mm: f64,
}

impl Default for RaisedSettings {
    fn default() -> Self {
        Self { width_mm: 1.2, clearance_mm: 4.0, beam_mm: 1.0, warn_mm: 40.0 }
    }
}

impl RaisedSettings {
    pub(super) fn check(self) -> Result<(), Error> {
        for (label, value, min, max) in [
            ("connector width", self.width_mm, 0.6, 5.0),
            ("connector clearance", self.clearance_mm, 0.4, 30.0),
            ("beam thickness", self.beam_mm, 0.4, 5.0),
            ("span warning", self.warn_mm, 1.0, 200.0),
        ] {
            if !(min..=max).contains(&value) {
                return Err(Error::Settings(format!("The {label} must be between {min} and {max} mm.")));
            }
        }
        Ok(())
    }

    pub(super) fn check_clearance(self, thickness: i32) -> Result<(), Error> {
        if um(self.clearance_mm) - thickness < STEP {
            return Err(Error::Settings(
                "The connector clearance from the work must be at least 0.2 mm above the top of the plate.".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Supports for one way of printing and painting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SupportKind {
    /// A bridge cut out of the opening at the work face.
    Flat,
    /// A beam held above the work by two feet.
    Raised,
}

/// The two outside ends of a support, in micrometres from the plate's bottom-left.
pub type Ends = [[i32; 2]; 2];

/// Remove or move a suggestion at these exact ends, or add a new support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct SupportEdit {
    /// Source paint, independent of the coat order and of any RAL match.
    pub paint: Colour,
    /// Manual changes to one kind do not alter suggestions for the other kind.
    pub kind: SupportKind,
    /// The suggestion to suppress, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<Ends>,
    /// The edited ends, if any. A removal has only the original ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends: Option<Ends>,
}

pub(super) fn check_edits(edits: &[SupportEdit]) -> Result<(), Error> {
    if edits.len() > MAX_SUPPORT_EDITS {
        return Err(Error::Settings(format!("A project may store at most {MAX_SUPPORT_EDITS} support edits.")));
    }
    for edit in edits {
        if edit.original.is_none() && edit.ends.is_none() {
            return Err(Error::Settings("A support edit needs original or new ends.".to_owned()));
        }
        if edit.original.into_iter().chain(edit.ends).flatten().flatten().any(|v| !(-(1 << 30)..=1 << 30).contains(&v))
        {
            return Err(Error::Settings("A support coordinate is too far from the plate.".to_owned()));
        }
    }
    Ok(())
}

/// Why an edited support cannot go on the plate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportIssue {
    /// The full landing must fit on solid material at each end.
    Landings,
    /// A support must cross one opening between two solid pieces.
    Span,
    /// The support would meet another manual support.
    Conflict,
    /// The saved removal no longer matches an automatic suggestion.
    Suggestion,
}

impl SupportIssue {
    /// Stable code for the interface to explain in its own language.
    pub fn code(self) -> &'static str {
        match self {
            Self::Landings => "landings",
            Self::Span => "span",
            Self::Conflict => "conflict",
            Self::Suggestion => "suggestion",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Beam {
    ends: Ends,
    length: f64,
    width: i32,
}

impl Beam {
    fn new(ends: Ends, width: i32) -> Self {
        let [[x0, y0], [x1, y1]] = ends;
        let length = libm::hypot(f64::from(x1) - f64::from(x0), f64::from(y1) - f64::from(y0));
        Self { ends, length, width }
    }

    fn section(self, from: f64, to: f64) -> Shape {
        let [[x0, y0], [x1, y1]] = self.ends;
        let ux = (f64::from(x1) - f64::from(x0)) / self.length;
        let uy = (f64::from(y1) - f64::from(y0)) / self.length;
        let half = f64::from(self.width / 2);
        let other = f64::from(self.width) - half;
        #[allow(clippy::cast_possible_truncation, reason = "the ends were checked against the plate's bounds")]
        let point = |along: f64, across: f64| {
            Point::new(
                (f64::from(x0) + ux * along - uy * across).round_ties_even() as i32,
                (f64::from(y0) + uy * along + ux * across).round_ties_even() as i32,
            )
        };
        vec![vec![point(from, -half), point(to, -half), point(to, other), point(from, other)]]
    }

    fn outline(self) -> Shape {
        self.section(0.0, self.length)
    }

    fn feet(self, depth: i32) -> [Shape; 2] {
        let depth = f64::from(depth);
        [self.section(0.0, depth), self.section(self.length - depth, self.length)]
    }

    fn span(self, landing: i32) -> i32 {
        #[allow(clippy::cast_possible_truncation, reason = "the checked span is shorter than the plate diagonal")]
        let span = (self.length - 2.0 * f64::from(landing)).round_ties_even() as i32;
        span
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Placed {
    beam: Beam,
    shape: Shape,
    pieces: [usize; 2],
    gap: i32,
    edit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct Unplaced {
    ends: Ends,
    edit: usize,
    issue: SupportIssue,
}

/// A suggested or manually edited support in the top view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SupportDrawing {
    /// Outer ends, in the project's coordinate system.
    pub ends: Ends,
    /// The projected beam or bridge, as a path in millimetres with y down.
    pub path: String,
    /// The free span of the beam, or the gap crossed by a flat bridge.
    pub span_mm: f64,
    /// Where the edit is stored in the project, if this is a manual support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<usize>,
    /// Why the support was left out of the plate, if it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<SupportIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Spec {
    pub(super) width: i32,
    pub(super) raised: Option<RaisedSettings>,
}

impl Spec {
    fn kind(self) -> SupportKind {
        if self.raised.is_some() { SupportKind::Raised } else { SupportKind::Flat }
    }

    fn landing(self) -> i32 {
        if self.raised.is_some() { self.width } else { self.width / 2 }
    }
}

/// Materials on which the ends are allowed to land.
pub(super) struct Surface<'a> {
    pub(super) base: &'a Shapes,
    pub(super) top: &'a Shapes,
    pub(super) size: (i32, i32),
}

impl Surface<'_> {
    fn check(&self, beam: Beam, landing: i32) -> Result<[usize; 2], SupportIssue> {
        let (w, h) = self.size;
        if beam.ends.iter().any(|&[x, y]| !(0..=w).contains(&x) || !(0..=h).contains(&y)) {
            return Err(SupportIssue::Landings);
        }
        if beam.span(landing) <= 2 * ALIGN_UM {
            return Err(SupportIssue::Span);
        }
        let [left, right] = beam.feet(landing);
        let anchor = |foot: Shape| {
            let foot = vec![foot];
            let (lo, hi) = bounds(&foot)?;
            self.base.iter().position(|piece| {
                let Some((a, b)) = bounds(std::slice::from_ref(piece)) else { return false };
                a.x <= lo.x
                    && a.y <= lo.y
                    && b.x >= hi.x
                    && b.y >= hi.y
                    && difference(&foot, &vec![piece.clone()]).is_empty()
                    && difference(&foot, self.top).is_empty()
            })
        };
        let (Some(a), Some(b)) = (anchor(left), anchor(right)) else { return Err(SupportIssue::Landings) };
        let outline = beam.outline();
        if a == b
            || intersection(&padded(outline.clone()), self.base).len() != 2
            || difference(&vec![outline], self.base).len() != 1
        {
            return Err(SupportIssue::Span);
        }
        Ok([a, b])
    }
}

/// Leave more than the mesher's snapping tolerance between neighbouring supports.
fn padded(shape: Shape) -> Shapes {
    let shapes = vec![shape];
    offset(&shapes, 3 * ALIGN_UM, Corners::Bevel).unwrap_or(shapes)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Supports {
    spec: Option<Spec>,
    placed: Vec<Placed>,
    unplaced: Vec<Unplaced>,
    pub(super) loose: Vec<usize>,
    pub(super) single: Vec<usize>,
}

impl Supports {
    pub(super) fn build(spec: Spec, surface: &Surface<'_>, paint: Rgb, edits: &[SupportEdit], chamfer: i32) -> Self {
        let mut supports = Self { spec: Some(spec), ..Self::default() };
        let kind = spec.kind();
        let relevant: Vec<_> =
            edits.iter().enumerate().filter(|(_, edit)| edit.paint.0 == paint && edit.kind == kind).collect();
        for &(i, edit) in &relevant {
            let Some(ends) = edit.ends else { continue };
            match supports.manual(spec, surface, ends, Some(i)) {
                Ok(placed) => supports.placed.push(placed),
                Err(issue) => supports.unplaced.push(Unplaced { ends, edit: i, issue }),
            }
        }

        let landing = spec.landing();
        let setback = if spec.raised.is_some() { 100 + chamfer } else { 0 };
        let (suggested, single) = suggestions(surface.base, surface.size, spec.width, landing + setback);
        let manual: Vec<_> = supports.placed.iter().map(|p| padded(p.shape.clone())).collect();
        let mut unmatched: BTreeSet<_> = relevant.iter().filter_map(|(_, edit)| edit.original).collect();
        for bridge in suggested {
            let ends = [bridge.end(0), bridge.end(1)].map(|p| [p.x, p.y]);
            if unmatched.remove(&ends) {
                continue;
            }
            let shape = bridge.shape();
            if manual.iter().any(|shape2| !intersection(&vec![shape.clone()], shape2).is_empty()) {
                continue;
            }
            let beam = Beam::new(ends, spec.width);
            if kind == SupportKind::Raised
                && beam.feet(landing).into_iter().any(|foot| !difference(&vec![foot], surface.top).is_empty())
            {
                continue;
            }
            let gap = if kind == SupportKind::Raised { beam.span(landing) } else { bridge.gap };
            supports.placed.push(Placed { beam, shape, pieces: bridge.pieces, gap, edit: None });
        }
        for (edit, saved) in &relevant {
            if let Some(ends) = saved.original.filter(|ends| unmatched.contains(ends)) {
                supports.unplaced.push(Unplaced { ends, edit: *edit, issue: SupportIssue::Suggestion });
            }
        }
        // Preserve the original flat-bridge report for existing projects.
        if kind == SupportKind::Flat && relevant.is_empty() {
            supports.single = single;
        } else {
            supports.check_pieces(surface.base.len());
        }
        supports
    }

    fn manual(
        &self,
        spec: Spec,
        surface: &Surface<'_>,
        ends: Ends,
        edit: Option<usize>,
    ) -> Result<Placed, SupportIssue> {
        let beam = Beam::new(ends, spec.width);
        let pieces = surface.check(beam, spec.landing())?;
        let shape = beam.outline();
        let expanded = padded(shape.clone());
        if self
            .placed
            .iter()
            .filter(|p| p.edit.is_some() && p.edit != edit)
            .any(|p| !intersection(&expanded, &vec![p.shape.clone()]).is_empty())
        {
            return Err(SupportIssue::Conflict);
        }
        Ok(Placed { beam, shape, pieces, gap: beam.span(spec.landing()), edit })
    }

    pub(super) fn issue(&self, surface: &Surface<'_>, ends: Ends, edit: Option<usize>) -> Option<SupportIssue> {
        self.spec.and_then(|spec| self.manual(spec, surface, ends, edit).err())
    }

    fn check_pieces(&mut self, count: usize) {
        if count < 2 {
            return;
        }
        let mut neighbours = vec![Vec::new(); count];
        for &[a, b] in self.placed.iter().map(|p| &p.pieces) {
            neighbours[a].push(b);
            neighbours[b].push(a);
        }
        let paths = |without| {
            let mut reached = vec![false; count];
            reached[0] = true;
            let mut queue = VecDeque::from([0]);
            while let Some(piece) = queue.pop_front() {
                for &next in &neighbours[piece] {
                    if !reached[next] && Some(next) != without {
                        reached[next] = true;
                        queue.push_back(next);
                    }
                }
            }
            reached
        };
        let connected = paths(None);
        for piece in 1..count {
            if !connected[piece] {
                self.loose.push(piece);
            } else if neighbours[piece].len() == 1 || {
                let can_reach_rim = paths(Some(piece));
                neighbours[piece].iter().filter(|&&next| can_reach_rim[next]).take(2).count() == 1
            } {
                self.single.push(piece);
            }
        }
    }

    pub(super) fn footprints(&self) -> Shapes {
        self.placed.iter().map(|p| p.shape.clone()).collect()
    }

    pub(super) fn count(&self) -> usize {
        self.placed.len()
    }

    pub(super) fn raised(&self) -> Option<RaisedSettings> {
        self.spec?.raised
    }

    pub(super) fn unplaced(&self) -> usize {
        self.unplaced.len() - self.unmatched_suggestions()
    }

    pub(super) fn unmatched_suggestions(&self) -> usize {
        self.unplaced.iter().filter(|item| item.issue == SupportIssue::Suggestion).count()
    }

    pub(super) fn long_spans(&self, warn: i32) -> usize {
        self.placed.iter().filter(|p| free_span(p.gap) > warn).count()
    }

    pub(super) fn height(&self) -> Option<i32> {
        let raised = self.spec?.raised?;
        self.placed.iter().map(|p| um(raised.clearance_mm) + reach(p.gap) + um(raised.beam_mm)).max()
    }

    pub(super) fn drawing(&self, height: i32) -> Vec<SupportDrawing> {
        self.placed
            .iter()
            .map(|p| SupportDrawing {
                ends: p.beam.ends,
                path: svg_path(std::slice::from_ref(&p.shape), height),
                span_mm: mm(if self.spec.is_some_and(|s| s.raised.is_some()) { free_span(p.gap) } else { p.gap }),
                edit: p.edit,
                issue: None,
            })
            .chain(self.unplaced.iter().map(|p| SupportDrawing {
                ends: p.ends,
                path: String::new(),
                span_mm: 0.0,
                edit: Some(p.edit),
                issue: Some(p.issue),
            }))
            .collect()
    }

    pub(super) fn extend_slabs(&self, slabs: &mut Vec<Slab>, thickness: i32) -> Result<(), Error> {
        let Some(spec) = self.spec else { return Ok(()) };
        let Some(raised) = spec.raised else { return Ok(()) };
        if self.placed.is_empty() {
            return Ok(());
        }
        raised.check_clearance(thickness)?;
        let clearance = um(raised.clearance_mm);
        let beam = um(raised.beam_mm);
        let mut levels = vec![thickness, clearance];
        for p in &self.placed {
            levels.extend((clearance..clearance + reach(p.gap)).step_by(STEP as usize));
            levels.extend([clearance + reach(p.gap), clearance + reach(p.gap) + beam]);
        }
        levels.sort_unstable();
        levels.dedup();
        for z in levels.windows(2) {
            let [z0, z1] = [z[0], z[1]];
            let mut shapes = Shapes::new();
            for p in &self.placed {
                let top = clearance + reach(p.gap);
                if (top..top + beam).contains(&z0) {
                    shapes.push(p.shape.clone());
                } else if z0 < top {
                    // Another connector's beam may end partway through this
                    // one's step. Its feet still grow on the 0.2 mm grid.
                    let extension = if z0 < clearance { 0 } else { (z0 - clearance) / STEP * STEP + STEP };
                    shapes.extend(p.beam.feet(spec.width + extension));
                }
            }
            slabs.push(Slab { z0, z1, shapes });
        }
        Ok(())
    }
}

fn reach(gap: i32) -> i32 {
    ((gap - SHORT_SPAN).max(0) / (2 * STEP) * STEP).min(MAX_REACH)
}

fn free_span(gap: i32) -> i32 {
    gap - 2 * reach(gap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_end_of_one_beam_does_not_add_a_step_to_another() {
        let raised = RaisedSettings { beam_mm: 0.9, ..RaisedSettings::default() };
        let width = um(raised.width_mm);
        let placed = |y, gap| {
            let beam = Beam::new([[0, y], [2 * width + gap, y]], width);
            Placed { beam, shape: beam.outline(), pieces: [0, 1], gap, edit: None }
        };
        let supports = Supports {
            spec: Some(Spec { width, raised: Some(raised) }),
            placed: vec![placed(0, 30_400), placed(4_000, 34_800)],
            ..Supports::default()
        };
        let mut slabs = Vec::new();
        supports.extend_slabs(&mut slabs, 1_200).unwrap();
        let at = |z| &slabs.iter().find(|slab| slab.z0 == z).unwrap().shapes;
        let long = supports.placed[1].beam;
        assert!(at(5_000).ends_with(&long.feet(2_400)));
        assert_eq!(at(5_100), &long.feet(2_400));
        assert_eq!(at(5_200), &long.feet(2_600));
    }
}
