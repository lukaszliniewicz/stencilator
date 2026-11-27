//! Spray-side finishing and overlap between coats.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::geometry::{
    Corners, Point, Shapes, clean, difference, intersection, offset, rectangle, translate, um, union,
};
use crate::mesh::Slab;
use crate::ral;
use crate::stencil::RECOMMENDED_MIN_WEB_MM;

use super::{Plate, PlateSettings, plate_solid};

const STEP_UM: i32 = 200;
const MARK_DEPTH_UM: i32 = 400;

/// The mark cut into the top of a plate's margin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    /// Leave the margin plain.
    #[default]
    None,
    /// The plate's number in the palette.
    Number,
    /// The four digits of its nearest RAL Classic match.
    Ral,
}

/// Optional treatment of the openings and the plate margin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FinishSettings {
    /// Depth and width of the stepped spray-side chamfer, in millimetres.
    pub chamfer_mm: f64,
    /// How far an earlier coat can extend under a later one, in millimetres.
    pub overlap_mm: f64,
    /// The optional debossed mark.
    pub mark: Mark,
    /// Report approximate RAL matches without changing the source palette.
    pub match_ral: bool,
}

impl FinishSettings {
    pub(crate) fn is_plain(&self) -> bool {
        *self == Self::default()
    }

    pub(super) fn check(&self) -> Result<(), Error> {
        for (name, value) in [("chamfer", self.chamfer_mm), ("paint overlap", self.overlap_mm)] {
            if value != 0.0 && !(0.1..=2.0).contains(&value) {
                return Err(Error::Settings(format!("The {name} must be zero or between 0.1 and 2 mm.")));
            }
        }
        Ok(())
    }

    pub(super) fn check_depth(&self, thickness: i32) -> Result<(), Error> {
        if self.chamfer_mm != 0.0 && thickness - um(self.chamfer_mm) < STEP_UM {
            return Err(Error::Settings(
                "The chamfer must leave at least 0.2 mm of plate at the work face.".to_owned(),
            ));
        }
        Ok(())
    }
}

/// The result of placing a debossed identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarkReport {
    /// Digits requested for the mark.
    pub text: String,
    /// Whether there was room and thickness for the mark.
    pub applied: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct Finish {
    /// Cross-sections, starting at these depths below the top (deepest first).
    sections: Vec<(i32, Shapes)>,
    pub(super) top_openings: Option<Shapes>,
    pub(super) engraving: Shapes,
    pub(super) mark: Option<MarkReport>,
    pub(super) ral: Option<ral::Match>,
    pub(super) chamfer_limited: bool,
}

impl Finish {
    pub(super) fn top_solid<'a>(&'a self, solid: &'a Shapes) -> &'a Shapes {
        self.sections.last().map_or(solid, |(_, shapes)| shapes)
    }

    pub(super) fn slabs(&self, solid: &Shapes, thickness: i32) -> Result<Vec<Slab>, Error> {
        let depth = self.sections.first().map_or(0, |(depth, _)| *depth);
        if thickness - depth < STEP_UM {
            return Err(Error::Settings("The plate is too thin for its finishing.".to_owned()));
        }
        let mut slabs = vec![Slab { z0: 0, z1: thickness - depth, shapes: solid.clone() }];
        for (i, (depth, shapes)) in self.sections.iter().enumerate() {
            let next = self.sections.get(i + 1).map_or(0, |(next, _)| *next);
            slabs.push(Slab { z0: thickness - depth, z1: thickness - next, shapes: shapes.clone() });
        }
        Ok(slabs)
    }
}

/// Extend an earlier coat into nearby openings of subsequent coats. Keep an extension
/// only when it joins the original opening, so overlap cannot jump a strip of bare work.
/// Report when the extension was limited to avoid leaving loose pieces or point contacts.
pub(super) fn trap(openings: &Shapes, later: &Shapes, reach: i32, min_web: i32) -> Result<(Shapes, bool), Error> {
    let widened = offset(openings, reach, Corners::Round)?;
    let extra = intersection(&widened, later);
    if extra.is_empty() {
        return Ok((openings.clone(), false));
    }
    let joined = |extra: &Shapes| {
        union(openings, extra)
            .into_iter()
            .filter(|shape| !intersection(&vec![shape.clone()], openings).is_empty())
            .collect()
    };
    let extended = joined(&extra);
    let original_islands = enclosed_material(openings);
    let original_contacts = boundary_contacts(openings);
    let material_to_keep = |extended: &Shapes| {
        // Separately traced paints may touch at one point or leave a hairline gap.
        // Keep the overlap away from those contacts and from material it would cut loose.
        let mut keep = difference(&enclosed_material(extended), &original_islands);
        if !original_islands.is_empty() {
            let added = difference(extended, openings);
            for island in &original_islands {
                if difference(&vec![island.clone()], &added).len() > 1 {
                    keep.push(island.clone());
                }
            }
        }
        for &Point { x, y } in boundary_contacts(extended).difference(&original_contacts) {
            keep.push(vec![rectangle(x - 1, y - 1, x + 1, y + 1)]);
        }
        keep
    };
    let keep = material_to_keep(&extended);
    if keep.is_empty() {
        return Ok((extended, false));
    }
    let reserve = offset(&keep, reach + min_web, Corners::Round)?;
    let limited = joined(&difference(&extra, &reserve));
    // Rounding at the new boundary can still create a point contact. In that
    // case keep the traced opening, which was printable before adding overlap.
    let openings = if material_to_keep(&limited).is_empty() { limited } else { openings.clone() };
    Ok((openings, true))
}

fn enclosed_material(openings: &Shapes) -> Shapes {
    let outlines = openings.iter().map(|shape| vec![shape[0].clone()]).collect();
    difference(&outlines, openings)
}

fn boundary_contacts(shapes: &Shapes) -> BTreeSet<Point> {
    let mut seen = BTreeSet::new();
    shapes.iter().flatten().flatten().copied().filter(|&point| !seen.insert(point)).collect()
}

impl Plate {
    pub(super) fn finish(
        &mut self,
        settings: &PlateSettings,
        body: (i32, i32),
        outline: &Shapes,
        unbridged: &Shapes,
        bridges: &Shapes,
    ) -> Result<(), Error> {
        let options = settings.finish;
        if options.match_ral || options.mark == Mark::Ral {
            self.finish.ral = Some(ral::nearest(self.colour));
        }
        let depth = um(options.chamfer_mm);
        let gap = um(settings.min_web_mm.max(RECOMMENDED_MIN_WEB_MM));
        let (width, height) = body;
        let allowed = if width.min(height) > 2 * gap {
            vec![vec![rectangle(gap, gap, width - gap, height - gap)]]
        } else {
            Vec::new()
        };
        let mut levels = Vec::new();
        for reach in (0..depth).step_by(STEP_UM as usize) {
            let grown = offset(unbridged, (reach + STEP_UM).min(depth), Corners::Bevel)?;
            let clipped = intersection(&grown, &allowed);
            self.finish.chamfer_limited |= !difference(&difference(&grown, &allowed), unbridged).is_empty();
            let openings = difference(&union(unbridged, &clipped), bridges);
            levels.push((depth - reach, openings));
        }
        self.finish.top_openings = levels.last().map(|(_, top)| top.clone());
        let top = self.finish.top_openings.as_ref().unwrap_or(&self.openings);
        let mark = match options.mark {
            Mark::None => None,
            Mark::Number => Some((self.paint + 1).to_string()),
            Mark::Ral => self.finish.ral.map(|matched| matched.code.to_string()),
        };
        if let Some(text) = mark {
            let thickness = um(settings.thickness_mm);
            let margin = um(settings.margin_mm);
            if thickness >= MARK_DEPTH_UM + STEP_UM {
                self.finish.engraving = engraving(&text, width, margin, gap, top);
            }
            let applied = !self.finish.engraving.is_empty();
            self.finish.mark = Some(MarkReport { text, applied });
            if applied {
                let i = levels.partition_point(|(level, _)| *level > MARK_DEPTH_UM);
                if levels.get(i).is_none_or(|(level, _)| *level != MARK_DEPTH_UM) {
                    let openings =
                        i.checked_sub(1).map_or_else(|| self.openings.clone(), |previous| levels[previous].1.clone());
                    levels.insert(i, (MARK_DEPTH_UM, openings));
                }
            }
        }
        self.finish.sections = levels
            .into_iter()
            .map(|(depth, openings)| {
                let mut solid = plate_solid(outline, &openings);
                if depth <= MARK_DEPTH_UM && !self.finish.engraving.is_empty() {
                    solid = difference(&solid, &self.finish.engraving);
                }
                (depth, solid)
            })
            .collect();
        Ok(())
    }
}

fn engraving(text: &str, width: i32, margin: i32, gap: i32, openings: &Shapes) -> Shapes {
    const DIGIT_WIDTH: i32 = 1_900;
    const DIGIT_HEIGHT: i32 = 3_100;
    const SPACE: i32 = 650;
    let digits = i32::try_from(text.len()).unwrap_or_default();
    let length = digits * (DIGIT_WIDTH + SPACE) - SPACE;
    if length + 2 * gap > width || DIGIT_HEIGHT + gap > margin {
        return Vec::new();
    }
    // Reserve clearance all round the digits. The mark always reads correctly
    // with the artwork upright, on the edge nearest the person using the plate.
    let reserved = vec![vec![rectangle(0, 0, length + 2 * gap, DIGIT_HEIGHT + 2 * gap)]];
    let right = width - length - 2 * gap;
    let anchor =
        [0, right / 2, right].into_iter().find(|&x| intersection(&translate(&reserved, x, 0), openings).is_empty());
    let Some(anchor) = anchor else { return Vec::new() };
    let mut shapes = Vec::new();
    for (i, byte) in (0..).zip(text.bytes()) {
        let x = anchor + gap + i * (DIGIT_WIDTH + SPACE);
        shapes.extend(translate(&digit(byte), x, gap));
    }
    clean(&shapes)
}

fn digit(value: u8) -> Shapes {
    // Seven segments, deliberately overlapped where they meet so a cut never
    // contacts another cut at a single point. Strokes are 0.5 mm wide.
    let segments = [
        (0, 2600, 1900, 3100),
        (1400, 1400, 1900, 3100),
        (1400, 0, 1900, 1700),
        (0, 0, 1900, 500),
        (0, 0, 500, 1700),
        (0, 1400, 500, 3100),
        (0, 1300, 1900, 1800),
    ];
    let bits = [
        0b011_1111, 0b000_0110, 0b101_1011, 0b100_1111, 0b110_0110, 0b110_1101, 0b111_1101, 0b000_0111, 0b111_1111,
        0b110_1111,
    ];
    let selected = bits[usize::from(value - b'0')];
    segments
        .into_iter()
        .enumerate()
        .filter(|(i, _)| selected & (1 << i) != 0)
        .map(|(_, (x0, y0, x1, y1))| vec![rectangle(x0, y0, x1, y1)])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::area_mm2;
    use crate::mesh::{inspect, mesh_slabs};

    #[test]
    fn overlap_does_not_cut_a_gap_at_a_traced_boundary_loose() {
        let earlier = vec![vec![rectangle(0, 0, 5000, 10_000)]];
        // Independent outlines cross, but there is a narrow gap in their middle.
        let later = vec![vec![vec![
            Point::new(4900, 0),
            Point::new(7000, 0),
            Point::new(7000, 10_000),
            Point::new(4900, 10_000),
            Point::new(4900, 6000),
            Point::new(5100, 6000),
            Point::new(5100, 4000),
            Point::new(4900, 4000),
        ]]];
        let (trapped, limited) = trap(&earlier, &later, 800, 600).unwrap();
        assert!(limited);
        assert_eq!(trapped.len(), 1);
        assert_eq!(trapped[0].len(), 1, "no detached piece in the original gap");
        let extra = difference(&trapped, &earlier);
        assert!(area_mm2(&extra) > 1.0);
        assert_eq!(difference(&extra, &later), Shapes::new());
    }

    #[test]
    fn overlap_does_not_make_openings_touch_at_one_point() {
        let earlier = vec![vec![rectangle(0, 0, 5000, 5000)], vec![rectangle(6000, 4000, 10_000, 8000)]];
        let later = vec![vec![rectangle(4000, 0, 6000, 4000)]];
        let (trapped, limited) = trap(&earlier, &later, 1500, 600).unwrap();
        assert!(limited);
        assert!(area_mm2(&trapped) > area_mm2(&earlier));
        let outline = vec![vec![rectangle(-1000, -1000, 12_000, 10_000)]];
        let mesh = mesh_slabs(&[Slab { z0: 0, z1: 1200, shapes: plate_solid(&outline, &trapped) }]).unwrap();
        let report = inspect(&mesh);
        assert!(report.is_closed() && report.pieces == 1, "{report:?}");
    }

    #[test]
    fn overlap_does_not_split_an_existing_island() {
        let island = vec![vec![rectangle(4000, 2000, 5400, 8000)]];
        let earlier = difference(&vec![vec![rectangle(0, 0, 10_000, 10_000)]], &island);
        let later = vec![vec![rectangle(0, 5000, 11_000, 5500)]];
        let (trapped, limited) = trap(&earlier, &later, 1000, 600).unwrap();
        assert!(limited);
        assert_eq!(enclosed_material(&trapped), enclosed_material(&earlier));
        assert!(area_mm2(&trapped) > area_mm2(&earlier));
    }
}
