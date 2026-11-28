//! An optional key on the plate and a separate frame that holds it against the work.

use serde::Serialize;

use crate::Error;
use crate::geometry::{BedFit, Contour, Point, Shapes, bed_fit, difference, mm, rectangle, translate, um, union};
use crate::mesh::{Mesh, Slab, mesh_slabs};
use crate::stencil::{check_thickness, encode_stl};

const KEY_DEPTH_UM: i32 = 3_000;
const RIM_UM: i32 = 5_000;
const GRIP_DEPTH_UM: i32 = 3_000;

/// The name of the frame in a set's print files.
pub const FRAME_NAME: &str = "registration-frame";
/// The name of the frame's STL in a folder or zip of print files.
pub const FRAME_FILE: &str = "registration-frame.stl";

/// The shared blank and its matching key pocket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Registration {
    body_um: (i32, i32),
    clearance_um: i32,
}

impl Registration {
    pub(crate) fn new(body_um: (i32, i32), clearance_um: i32) -> Result<Self, Error> {
        let registration = Self { body_um, clearance_um };
        let (w, h) = registration.frame_size_um();
        if w.max(h) >= 1 << 30 {
            return Err(Error::Geometry("The registration frame would be too large to work with.".to_owned()));
        }
        Ok(registration)
    }

    pub(crate) fn plate_size_um(self) -> (i32, i32) {
        (self.body_um.0, self.body_um.1 + KEY_DEPTH_UM)
    }

    fn frame_size_um(self) -> (i32, i32) {
        let (w, h) = self.plate_size_um();
        let extra = 2 * (RIM_UM + self.clearance_um);
        (w + extra, h + extra)
    }

    fn key(self) -> (i32, i32) {
        let w = self.body_um.0;
        let start = (w / 8).min(5_000);
        (start, start + (w / 4).min(8_000))
    }

    /// Move the keyed blank's edges out by `gap` to make its matching pocket.
    fn profile(self, gap: i32) -> Contour {
        let (w, h) = self.body_um;
        let (left, right) = self.key();
        let top = h + gap;
        vec![
            Point::new(-gap, -gap),
            Point::new(w + gap, -gap),
            Point::new(w + gap, top),
            Point::new(right + gap, top),
            Point::new(right + gap, top + KEY_DEPTH_UM),
            Point::new(left - gap, top + KEY_DEPTH_UM),
            Point::new(left - gap, top),
            Point::new(-gap, top),
        ]
    }

    pub(crate) fn outline(self) -> Shapes {
        vec![vec![self.profile(0)]]
    }

    fn pocket(self) -> Shapes {
        let (left, right) = self.key();
        let centre = left.midpoint(right);
        let half_grip = (self.body_um.0 / 4).min(6_000);
        let tip = self.plate_size_um().1 + self.clearance_um;
        // Leave room for a nail or pick at the tab's end without loosening its
        // shoulders or cutting all the way through the frame.
        let grip = rectangle(centre - half_grip, tip - 500, centre + half_grip, tip + GRIP_DEPTH_UM);
        union(&vec![vec![self.profile(self.clearance_um)]], &vec![vec![grip]])
    }

    pub(crate) fn frame(self) -> Frame {
        let size_um = self.frame_size_um();
        let outer = vec![vec![rectangle(0, 0, size_um.0, size_um.1)]];
        let shift = RIM_UM + self.clearance_um;
        let solid = difference(&outer, &translate(&self.pocket(), shift, shift));
        Frame { size_um, solid, clearance_um: self.clearance_um }
    }
}

/// The reusable frame for a set of keyed plates.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Width and height in micrometres, including the entire rim.
    pub size_um: (i32, i32),
    /// Frame material, in micrometres from its bottom-left corner.
    pub solid: Shapes,
    clearance_um: i32,
}

/// The frame's size and its fit on the print bed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FrameReport {
    /// Overall width in millimetres.
    pub width_mm: f64,
    /// Overall height in millimetres.
    pub height_mm: f64,
    /// Gap from the plate to the frame at each edge, in millimetres.
    pub clearance_mm: f64,
    /// Whether the frame fits the bed, turned if need be.
    pub fits_bed: bool,
    /// How the frame fits the bed: as drawn, after a quarter-turn, or not at all.
    pub bed_fit: BedFit,
    /// The STL file name in a set.
    pub file: &'static str,
}

impl FrameReport {
    /// The report as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

impl Frame {
    /// The frame as a closed solid, at the same thickness as its plates.
    ///
    /// # Errors
    ///
    /// [`Error::Stencil`] for an out-of-range thickness, or [`Error::Geometry`] if
    /// the mesh cannot be closed.
    pub fn mesh(&self, thickness_mm: f64) -> Result<Mesh, Error> {
        check_thickness(thickness_mm)?;
        mesh_slabs(&[Slab { z0: 0, z1: um(thickness_mm), shapes: self.solid.clone() }])
    }

    /// The frame as binary STL in millimetres.
    ///
    /// # Errors
    ///
    /// As [`Frame::mesh`].
    pub fn stl(&self, thickness_mm: f64) -> Result<Vec<u8>, Error> {
        Ok(encode_stl(&self.mesh(thickness_mm)?.triangles_mm(), "registration frame"))
    }

    /// The frame's dimensions and nominal clearance, and whether it fits the bed.
    pub fn report(&self, bed_mm: (f64, f64)) -> FrameReport {
        let (width_mm, height_mm) = (mm(self.size_um.0), mm(self.size_um.1));
        let fit = bed_fit((width_mm, height_mm), bed_mm);
        FrameReport {
            width_mm,
            height_mm,
            clearance_mm: mm(self.clearance_um),
            fits_bed: fit.fits(),
            bed_fit: fit,
            file: FRAME_FILE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{area_mm2, bounds, intersection};
    use crate::mesh::inspect;

    #[test]
    fn the_frame_fits_the_keyed_blank_with_clearance_and_room_to_lift_it() {
        let registration = Registration::new((90_000, 70_000), 300).unwrap();
        let frame = registration.frame();
        assert_eq!(registration.plate_size_um(), (90_000, 73_000));
        assert_eq!(frame.size_um, (100_600, 83_600));
        assert!(!frame.report((95.0, 90.0)).fits_bed);
        assert_eq!(frame.report((95.0, 90.0)).bed_fit, BedFit::TooLarge);
        assert!(frame.report((84.0, 101.0)).fits_bed, "the frame fits when turned");
        assert_eq!(frame.report((84.0, 101.0)).bed_fit, BedFit::QuarterTurn);
        assert_eq!(frame.report((101.0, 84.0)).bed_fit, BedFit::AsDrawn);
        let blank = translate(&registration.outline(), 5_300, 5_300);
        assert_eq!(intersection(&frame.solid, &blank), Shapes::new());
        let pocket = registration.pocket();
        // An 8 x 3 mm key with 0.3 mm clearance at its shoulders. The grip at
        // the tip leaves 2 mm of material to the outside of the rim.
        assert_eq!(intersection(&pocket, &vec![vec![rectangle(4699, 70_301, 4700, 72_800)]]), Shapes::new());
        assert_eq!(bounds(&pocket), Some((Point::new(-300, -300), Point::new(90_300, 76_300))));
        let mesh = inspect(&frame.mesh(1.2).unwrap());
        assert!(mesh.is_closed() && mesh.pieces == 1, "{mesh:?}");
        assert!((mesh.volume_mm3 - area_mm2(&frame.solid) * 1.2).abs() < 1e-6, "{mesh:?}");
        assert_eq!(mesh.max_mm, [100.6, 83.6, 1.2]);
    }

    #[test]
    fn the_key_rejects_rotated_and_flipped_plates_even_on_small_square_blanks() {
        for side in [4_050, 12_000, 90_000] {
            let registration = Registration::new((side, side), 600).unwrap();
            let pocket = registration.pocket();
            for turned in 0..4 {
                for flipped in [false, true] {
                    let mut shape = registration.outline();
                    for p in shape.iter_mut().flatten().flatten() {
                        if flipped {
                            p.x = side - p.x;
                        }
                        for _ in 0..turned {
                            *p = Point::new(side - p.y, p.x);
                        }
                    }
                    // At the extrema of plate movement inside the frame; the
                    // wrong-facing key is still caught even at the closest side.
                    for dx in [-600, 0, 600] {
                        for dy in [-600, 0, 600] {
                            let fits = difference(&translate(&shape, dx, dy), &pocket).is_empty();
                            assert_eq!(
                                fits,
                                turned == 0 && !flipped,
                                "side {side}, turns {turned}, flipped {flipped}, offset {dx}, {dy}"
                            );
                        }
                    }
                }
            }
        }
    }
}
