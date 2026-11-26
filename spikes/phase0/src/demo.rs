//! Demo geometry for spike 2: a 60 x 40 mm plate with a ring-shaped opening.
//! The disc inside the ring is an island, held by three raised connectors.
//!
//! Slabs, bottom up (the bottom touches the work):
//!
//! 1. 0.0-0.6 mm: plate with openings at their nominal size;
//! 2. 0.6-1.2 mm: openings grown by 0.3 mm (a chamfer step);
//! 3. 1.2-4.0 mm: connector feet, on the plate and on the island;
//! 4. 4.0-5.0 mm: connector beams, 4 mm above the work, bridging the opening.
//!
//! A slot 0.2 mm from the bottom edge grows through that edge in slab 2, which
//! forces intersections between neighbouring slabs. The whole design can be
//! rotated, so those intersections fall off the integer grid.

use std::f64::consts::TAU;

use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::mesh::int::outline::offset::IntOutlineOffset;
use i_overlay::mesh::int::style::IntOutlineStyle;

use crate::mesh::{Pt, Shapes, Slab, boolean, clean};

const CENTRE: (f64, f64) = (30.0, 20.0);
const RING_OUTER_MM: f64 = 12.0;
const RING_INNER_MM: f64 = 6.0;
const CHAMFER_UM: i32 = 300;
const CIRCLE_SEGMENTS: usize = 96;
const CONNECTOR_ANGLES_DEG: [f64; 3] = [30.0, 150.0, 270.0];

struct Frame {
    cos: f64,
    sin: f64,
}

impl Frame {
    fn new(rotation_deg: f64) -> Self {
        let r = rotation_deg.to_radians();
        Self { cos: r.cos(), sin: r.sin() }
    }

    /// Millimetres in design space to integer micrometres, rotated about the centre.
    fn point(&self, x: f64, y: f64) -> Pt {
        let (dx, dy) = (x - CENTRE.0, y - CENTRE.1);
        let rx = CENTRE.0 + dx * self.cos - dy * self.sin;
        let ry = CENTRE.1 + dx * self.sin + dy * self.cos;
        Pt::new((rx * 1000.0).round() as i32, (ry * 1000.0).round() as i32)
    }

    fn polygon(&self, points: &[(f64, f64)]) -> Vec<Pt> {
        points.iter().map(|&(x, y)| self.point(x, y)).collect()
    }

    fn circle(&self, radius: f64, clockwise: bool) -> Vec<Pt> {
        let mut points: Vec<(f64, f64)> = (0..CIRCLE_SEGMENTS)
            .map(|i| {
                let a = TAU * i as f64 / CIRCLE_SEGMENTS as f64;
                (CENTRE.0 + radius * a.cos(), CENTRE.1 + radius * a.sin())
            })
            .collect();
        if clockwise {
            points.reverse();
        }
        self.polygon(&points)
    }

    fn rect(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
        self.polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
    }

    /// A bar along the ray at `angle_deg` from the centre, between two radii.
    fn radial_bar(&self, angle_deg: f64, r0: f64, r1: f64, width: f64) -> Vec<Pt> {
        let a = angle_deg.to_radians();
        let (ux, uy) = (a.cos(), a.sin());
        let (nx, ny) = (-uy * width / 2.0, ux * width / 2.0);
        let at = |r: f64, side: f64| (CENTRE.0 + ux * r + nx * side, CENTRE.1 + uy * r + ny * side);
        self.polygon(&[at(r0, -1.0), at(r1, -1.0), at(r1, 1.0), at(r0, 1.0)])
    }
}

/// The four slabs of the demo plate.
pub fn connector_plate(rotation_deg: f64) -> Result<Vec<Slab>, String> {
    let f = Frame::new(rotation_deg);
    let plate: Shapes = vec![vec![f.rect(0.0, 0.0, 60.0, 40.0)]];
    let openings: Shapes =
        vec![vec![f.circle(RING_OUTER_MM, false), f.circle(RING_INNER_MM, true)], vec![f.rect(5.0, 0.2, 10.0, 3.0)]];
    let bottom = boolean(&plate, &openings, OverlayRule::Difference);
    let grown = clean(&openings).outline(&IntOutlineStyle::new(CHAMFER_UM)).map_err(|error| error.to_string())?;
    let chamfered = boolean(&plate, &grown, OverlayRule::Difference);

    let mut feet: Shapes = Vec::new();
    let mut beams: Shapes = Vec::new();
    for angle in CONNECTOR_ANGLES_DEG {
        feet.push(vec![f.radial_bar(angle, 3.8, 5.0, 1.0)]); // on the island
        feet.push(vec![f.radial_bar(angle, 12.9, 14.2, 1.0)]); // on the plate
        beams.push(vec![f.radial_bar(angle, 3.5, 14.5, 1.2)]);
    }
    Ok(vec![
        Slab { z0: 0, z1: 600, shapes: bottom },
        Slab { z0: 600, z1: 1200, shapes: chamfered },
        Slab { z0: 1200, z1: 4000, shapes: feet },
        Slab { z0: 4000, z1: 5000, shapes: beams },
    ])
}
