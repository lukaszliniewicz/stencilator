//! Free-form plates: one per paint, each the artwork's rectangle with a margin around it,
//! less the openings where that paint goes.
//!
//! Every plate has the same outline, optionally keyed to fit a registration frame. The
//! openings are the paint's cells traced into smooth outlines ([`crate::trace`]). A plate
//! is one slab, meshed into a closed solid. Flat bridges can hold parts of it cut off from
//! the rim. Any pieces still loose stay in the mesh, and [`check_plate`] reports them with
//! everything else that may not print or paint well.
//!
//! Plate coordinates are in micrometres from the plate's bottom-left corner, y up, with
//! the artwork the right way up seen from above.

use std::ops::RangeInclusive;

use serde::{Serialize, Serializer};

use crate::Error;
use crate::artwork::LabelMap;
use crate::bridges::flat_bridges;
use crate::colour::Rgb;
use crate::geometry::{
    Corners, Point, Shape, Shapes, UM_PER_MM, area_mm2, bounds, difference, fits_bed, mm, offset, rectangle, svg_path,
    translate, um,
};
use crate::mesh::{Mesh, Slab, mesh_slabs};
use crate::registration::{FRAME_FILE, FRAME_NAME, Frame, Registration};
use crate::stencil::{RECOMMENDED_MIN_WEB_MM, check_thickness, encode_stl};
use crate::threemf::{Object, write_3mf};
use crate::trace::{TraceSettings, trace_mask};

/// Accepted margins around the artwork, in millimetres.
pub const MARGIN_MM: RangeInclusive<f64> = 2.0..=100.0;

/// Accepted narrowest webs and openings, in millimetres.
pub const FEATURE_MM: RangeInclusive<f64> = 0.1..=5.0;

/// Accepted print bed sides, in millimetres.
pub const BED_MM: RangeInclusive<f64> = 50.0..=2000.0;

/// Accepted flat bridge widths in millimetres.
pub const BRIDGE_MM: RangeInclusive<f64> = RECOMMENDED_MIN_WEB_MM..=5.0;

/// Accepted gaps between a keyed plate and the frame at each edge, in millimetres.
pub const FRAME_CLEARANCE_MM: RangeInclusive<f64> = 0.1..=0.6;

/// Space between plates laid out in a 3MF file, in micrometres.
const LAYOUT_GAP_UM: i32 = 10_000;

/// Narrow parts thinner than twice this, in micrometres, are rounding left over from the
/// polygon engine, not features: printers cannot resolve them, and tracing makes nothing
/// that thin.
const SLIVER_UM: i32 = 10;

/// How plates are made and checked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlateSettings {
    /// Thickness of the plates in millimetres.
    pub thickness_mm: f64,
    /// Solid border around the artwork, in millimetres.
    pub margin_mm: f64,
    /// Plate material narrower than this, in millimetres, is reported: it prints badly and
    /// breaks.
    pub min_web_mm: f64,
    /// Openings narrower than this, in millimetres, are reported: the printer may close
    /// them and little paint gets through.
    pub min_opening_mm: f64,
    /// Width and depth of the print bed in millimetres; larger plates are reported.
    pub bed_mm: (f64, f64),
    /// Width of flat bridges in millimetres, or `None` to leave the openings as traced.
    pub bridge_width_mm: Option<f64>,
    /// Gap at each edge between the plate and the frame, or `None` for plain plates.
    pub registration_clearance_mm: Option<f64>,
}

impl Default for PlateSettings {
    fn default() -> Self {
        PlateSettings {
            thickness_mm: 1.2,
            margin_mm: 10.0,
            min_web_mm: RECOMMENDED_MIN_WEB_MM,
            min_opening_mm: 0.5,
            bed_mm: (256.0, 256.0),
            bridge_width_mm: None,
            registration_clearance_mm: None,
        }
    }
}

impl PlateSettings {
    /// Checks every setting against its accepted range.
    ///
    /// # Errors
    ///
    /// [`Error::Settings`] naming the first setting out of range, and the range, or
    /// [`Error::Stencil`] for the thickness, as in tile mode.
    pub fn check(&self) -> Result<(), Error> {
        check_thickness(self.thickness_mm)?;
        let mm = |name: &str, value: f64, range: &RangeInclusive<f64>| {
            if range.contains(&value) {
                Ok(())
            } else {
                Err(Error::Settings(format!("{name} must be between {} and {} mm.", range.start(), range.end())))
            }
        };
        mm("The margin", self.margin_mm, &MARGIN_MM)?;
        mm("The narrowest web", self.min_web_mm, &FEATURE_MM)?;
        mm("The narrowest opening", self.min_opening_mm, &FEATURE_MM)?;
        mm("The bed width", self.bed_mm.0, &BED_MM)?;
        mm("The bed depth", self.bed_mm.1, &BED_MM)?;
        if let Some(width) = self.bridge_width_mm {
            mm("The flat bridge width", width, &BRIDGE_MM)?;
        }
        if let Some(clearance) = self.registration_clearance_mm {
            mm("The registration clearance", clearance, &FRAME_CLEARANCE_MM)?;
        }
        Ok(())
    }
}

/// One paint's plate.
#[derive(Debug, Clone, PartialEq)]
pub struct Plate {
    /// The paint's place in the label map's palette, from 0.
    pub paint: usize,
    /// How many paints the palette has, which sets the width of the number in file names.
    pub paints: usize,
    /// The paint's colour.
    pub colour: Rgb,
    /// Width and height in micrometres.
    pub size_um: (i32, i32),
    /// Area of the paint's cells in square millimetres.
    pub paint_mm2: f64,
    /// Where paint goes through.
    pub openings: Shapes,
    /// The plate itself: its outline less the openings. The first shape is the rim, the
    /// rest are islands.
    pub solid: Shapes,
    /// What the optional flat bridges changed.
    pub flat_bridges: Option<FlatBridgeReport>,
    registration: Option<Registration>,
}

/// A place on a plate: the rectangle around it, in millimetres from the plate's
/// bottom-left corner, and its area.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Spot {
    /// Left edge.
    #[serde(serialize_with = "thousandths")]
    pub x_mm: f64,
    /// Bottom edge.
    #[serde(serialize_with = "thousandths")]
    pub y_mm: f64,
    /// Width.
    #[serde(serialize_with = "thousandths")]
    pub width_mm: f64,
    /// Height.
    #[serde(serialize_with = "thousandths")]
    pub height_mm: f64,
    /// Area of the part itself.
    #[serde(serialize_with = "thousandths")]
    pub area_mm2: f64,
}

/// What changed when flat bridges were requested.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlatBridgeReport {
    /// Number of bridges added to the plate.
    pub count: usize,
    /// Area of the opening covered by bridges, in square millimetres.
    #[serde(serialize_with = "thousandths")]
    pub covered_mm2: f64,
    /// Islands with just one bridge towards the rim, which may still be fragile.
    pub single_bridge_islands: Vec<Spot>,
}

/// What may not print or paint well on one plate. Warnings only: the plate is made anyway.
///
/// As JSON, lengths and areas are rounded to thousandths and the colour is `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlateReport {
    /// The paint's place in the palette, from 0.
    pub paint: usize,
    /// The paint's colour.
    #[serde(serialize_with = "hex")]
    pub colour: Rgb,
    /// Width of the plate.
    #[serde(serialize_with = "thousandths")]
    pub width_mm: f64,
    /// Height of the plate.
    #[serde(serialize_with = "thousandths")]
    pub height_mm: f64,
    /// Separate openings.
    pub openings: usize,
    /// Area of the paint's cells.
    #[serde(serialize_with = "thousandths")]
    pub paint_mm2: f64,
    /// Area of the traced openings. Far below the cells' area when much of the paint is in
    /// detail finer than tracing keeps.
    #[serde(serialize_with = "thousandths")]
    pub opening_mm2: f64,
    /// Pieces of plate cut off from the rim, which fall out unless something holds them.
    pub islands: Vec<Spot>,
    /// Plate material narrower than [`PlateSettings::min_web_mm`] for longer than that:
    /// it prints badly and breaks.
    pub thin_webs: Vec<Spot>,
    /// Openings narrower than [`PlateSettings::min_opening_mm`] for longer than that: the
    /// printer may close them, and little paint gets through.
    pub narrow_openings: Vec<Spot>,
    /// Whether the plate fits the bed, turned if need be.
    pub fits_bed: bool,
    /// What the optional flat bridges changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flat_bridges: Option<FlatBridgeReport>,
}

/// A report with its plate's file name, as the command line prints it and the app reads it.
#[derive(Serialize)]
struct Summary<'a> {
    stencil: String,
    #[serde(flatten)]
    report: &'a PlateReport,
}

/// The reports of `plates` as a JSON array, each with its plate's file name under
/// `stencil`: one form for the command line's `--json` and for the app.
pub fn reports_json(plates: &[Plate], reports: &[PlateReport]) -> serde_json::Value {
    let summaries: Vec<Summary> =
        plates.iter().zip(reports).map(|(plate, report)| Summary { stencil: plate.file_name(), report }).collect();
    serde_json::to_value(summaries).unwrap_or_default()
}

/// Lengths and areas to the thousandth; finer is noise from floating point.
#[allow(clippy::trivially_copy_pass_by_ref, reason = "serde passes fields by reference")]
fn thousandths<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64((value * 1000.0).round() / 1000.0)
}

/// A colour as `#rrggbb`.
#[allow(clippy::trivially_copy_pass_by_ref, reason = "serde passes fields by reference")]
fn hex<S: Serializer>(colour: &Rgb, serializer: S) -> Result<S::Ok, S::Error> {
    let [r, g, b] = colour;
    serializer.collect_str(&format_args!("#{r:02x}{g:02x}{b:02x}"))
}

/// The plate of every paint not in `skip`, in palette order. Skipped paints are those
/// nobody paints, such as the canvas colour; the others keep their numbers. If every
/// paint is skipped, there are no plates.
///
/// # Errors
///
/// [`Error::Settings`] or [`Error::Stencil`] when a setting is out of range, and
/// [`Error::Geometry`] when the plates would be too large to work with.
pub fn build_plates(map: &LabelMap, settings: &PlateSettings, skip: &[Rgb]) -> Result<Vec<Plate>, Error> {
    settings.check()?;
    if map.palette.iter().all(|colour| skip.contains(colour)) {
        return Ok(Vec::new());
    }
    let too_large = || Error::Geometry("The plates would be too large to work with.".to_owned());
    let side = |cells: u32| i32::try_from(u64::from(cells) * u64::from(map.cell_um)).map_err(|_| too_large());
    let margin = um(settings.margin_mm);
    let grown = |side: i32| side.checked_add(2 * margin).filter(|&s| s < 1 << 30).ok_or_else(too_large);
    let body = (grown(side(map.width)?)?, grown(side(map.height)?)?);
    let registration =
        settings.registration_clearance_mm.map(|clearance| Registration::new(body, um(clearance))).transpose()?;
    let size_um = registration.map_or(body, Registration::plate_size_um);
    let outline = registration.map_or_else(|| vec![vec![rectangle(0, 0, body.0, body.1)]], Registration::outline);
    let trace = TraceSettings { cell_um: map.cell_um, ..TraceSettings::default() };
    let cell_mm = f64::from(map.cell_um) / UM_PER_MM;
    let cell_mm2 = cell_mm * cell_mm;
    let counts = map.cell_counts();

    map.palette
        .iter()
        .enumerate()
        .filter(|(_, colour)| !skip.contains(colour))
        .map(|(paint, &colour)| {
            let openings = translate(&trace_mask(&map.mask(paint), map.width, map.height, &trace)?, margin, margin);
            #[allow(clippy::cast_precision_loss, reason = "at most 16 million cells")]
            let paint_mm2 = counts[paint] as f64 * cell_mm2;
            let solid = plate_solid(&outline, &openings);
            let mut plate = Plate {
                paint,
                paints: map.palette.len(),
                colour,
                size_um,
                paint_mm2,
                openings,
                solid,
                flat_bridges: None,
                registration,
            };
            if let Some(width) = settings.bridge_width_mm {
                plate.add_flat_bridges(&outline, um(width));
            }
            Ok(plate)
        })
        .collect()
}

fn plate_solid(outline: &Shapes, openings: &Shapes) -> Shapes {
    let mut solid = difference(outline, openings);
    // The rim is the piece that keeps the outline's corner.
    if let Some(rim) = solid.iter().position(|shape| shape[0].contains(&Point::new(0, 0))) {
        solid[..=rim].rotate_right(1);
    }
    solid
}

impl Plate {
    fn add_flat_bridges(&mut self, outline: &Shapes, width: i32) {
        let (bridges, singly_held) = flat_bridges(&self.solid, self.size_um, width);
        let single_bridge_islands = singly_held.into_iter().filter_map(|i| spot(&self.solid[i])).collect();
        let mut covered_mm2 = 0.0;
        if !bridges.is_empty() {
            let openings = difference(&self.openings, &bridges);
            covered_mm2 = (area_mm2(&self.openings) - area_mm2(&openings)).max(0.0);
            self.solid = plate_solid(outline, &openings);
            self.openings = openings;
        }
        self.flat_bridges = Some(FlatBridgeReport { count: bridges.len(), covered_mm2, single_bridge_islands });
    }

    /// The name, such as `plate-03-c02f34`: the paint's number counted from 1, padded so
    /// that the names sort in palette order, then its colour.
    pub fn name(&self) -> String {
        let [r, g, b] = self.colour;
        let width = self.paints.to_string().len().max(2);
        format!("plate-{:0width$}-{r:02x}{g:02x}{b:02x}", self.paint + 1)
    }

    /// The STL file name: the [`Plate::name`] with `.stl`.
    pub fn file_name(&self) -> String {
        format!("{}.stl", self.name())
    }

    /// The plate as a closed solid, from z = 0 to the thickness.
    ///
    /// # Errors
    ///
    /// [`Error::Stencil`] when the thickness is out of range, [`Error::Geometry`] when the
    /// mesh cannot be closed, which would be a fault in the engine.
    pub fn mesh(&self, thickness_mm: f64) -> Result<Mesh, Error> {
        check_thickness(thickness_mm)?;
        mesh_slabs(&[Slab { z0: 0, z1: um(thickness_mm), shapes: self.solid.clone() }])
    }

    /// The plate as binary STL in millimetres.
    ///
    /// # Errors
    ///
    /// As [`Plate::mesh`].
    pub fn stl(&self, thickness_mm: f64) -> Result<Vec<u8>, Error> {
        let [r, g, b] = self.colour;
        Ok(encode_stl(&self.mesh(thickness_mm)?.triangles_mm(), &format!("plate rgb({r}, {g}, {b})")))
    }
}

/// The matching frame when registration was requested for this set of plates.
pub fn registration_frame(plates: &[Plate]) -> Option<Frame> {
    plates.first()?.registration.map(Registration::frame)
}

/// Every plate as binary STL with its [`Plate::file_name`] and the optional frame, ready
/// to be zipped.
///
/// # Errors
///
/// As [`Plate::mesh`].
pub fn plate_files(plates: &[Plate], thickness_mm: f64) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let mut files = plates
        .iter()
        .map(|plate| Ok((plate.file_name(), plate.stl(thickness_mm)?)))
        .collect::<Result<Vec<_>, Error>>()?;
    if let Some(frame) = registration_frame(plates) {
        files.push((FRAME_FILE.to_owned(), frame.stl(thickness_mm)?));
    }
    Ok(files)
}

/// Every plate in one 3MF file, each an object named as [`Plate::name`] in its paint's
/// colour, followed by the optional registration frame. They lie in a grid with at least
/// 10 mm between them, in palette order from the top left.
///
/// # Errors
///
/// As [`Plate::mesh`], and [`Error::Geometry`] when the grid would be too large to work
/// with.
pub fn plates_3mf(plates: &[Plate], thickness_mm: f64) -> Result<Vec<u8>, Error> {
    let meshes = plates.iter().map(|plate| plate.mesh(thickness_mm)).collect::<Result<Vec<_>, _>>()?;
    let names: Vec<String> = plates.iter().map(Plate::name).collect();
    let frame = registration_frame(plates);
    let frame_mesh = frame.as_ref().map(|frame| frame.mesh(thickness_mm)).transpose()?;
    let count = plates.len() + usize::from(frame.is_some());
    let columns = (1..=count).find(|c| c * c >= count).unwrap_or(1);
    let rows = count.div_ceil(columns);
    let (width, height) = plates
        .iter()
        .map(|plate| plate.size_um)
        .chain(frame.iter().map(|frame| frame.size_um))
        .fold((0, 0), |(w, h), (p_w, p_h)| (w.max(p_w), h.max(p_h)));
    let too_large = || Error::Geometry("The plates would be too large to lay out together.".to_owned());
    let place = |index: usize, step: i32| {
        i32::try_from(index).ok().and_then(|i| i.checked_mul(step.checked_add(LAYOUT_GAP_UM)?)).ok_or_else(too_large)
    };
    let position = |i| Ok((place(i % columns, width)?, place(rows - 1 - i / columns, height)?));
    let mut objects = plates
        .iter()
        .zip(&meshes)
        .zip(&names)
        .enumerate()
        .map(|(i, ((plate, mesh), name))| Ok(Object { name, colour: plate.colour, mesh, position_um: position(i)? }))
        .collect::<Result<Vec<_>, Error>>()?;
    if let Some(mesh) = &frame_mesh {
        objects.push(Object { name: FRAME_NAME, colour: [128; 3], mesh, position_um: position(plates.len())? });
    }
    write_3mf(&objects)
}

/// What may not print or paint well on a plate.
///
/// # Errors
///
/// [`Error::Settings`] or [`Error::Stencil`] when a setting is out of range, and
/// [`Error::Geometry`] when the plate is too large to work with.
pub fn check_plate(plate: &Plate, settings: &PlateSettings) -> Result<PlateReport, Error> {
    inspect_plate(plate, settings).map(|(report, _)| report)
}

/// The shapes a plate's report describes, in the plate's coordinates, for drawing them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlateProblems {
    /// Pieces of plate cut off from the rim.
    pub islands: Shapes,
    /// Plate material too narrow to print well.
    pub thin_webs: Shapes,
    /// Openings too narrow to print well.
    pub narrow_openings: Shapes,
}

/// What may not print or paint well on a plate, as [`check_plate`] reports it, with the
/// shapes the report describes.
///
/// # Errors
///
/// As [`check_plate`].
pub fn inspect_plate(plate: &Plate, settings: &PlateSettings) -> Result<(PlateReport, PlateProblems), Error> {
    settings.check()?;
    let (width_mm, height_mm) = (mm(plate.size_um.0), mm(plate.size_um.1));
    let placed = |shapes: Shapes| -> (Shapes, Vec<Spot>) {
        shapes.into_iter().filter_map(|shape| spot(&shape).map(|s| (shape, s))).unzip()
    };
    // A narrow part no longer than the limit is a blunted corner, or the point where two
    // shapes touch diagonally: it neither breaks nor closes up.
    let long = |shapes: Shapes, limit: f64| {
        placed(
            shapes
                .into_iter()
                .filter(|shape| spot(shape).is_some_and(|s| s.width_mm.max(s.height_mm) > limit))
                .collect(),
        )
    };
    let (islands, island_spots) = placed(plate.solid.get(1..).unwrap_or_default().to_vec());
    let (thin_webs, web_spots) = long(narrow_parts(&plate.solid, um(settings.min_web_mm))?, settings.min_web_mm);
    let (narrow_openings, narrow_spots) =
        long(narrow_parts(&plate.openings, um(settings.min_opening_mm))?, settings.min_opening_mm);
    let report = PlateReport {
        paint: plate.paint,
        colour: plate.colour,
        width_mm,
        height_mm,
        openings: plate.openings.len(),
        paint_mm2: plate.paint_mm2,
        opening_mm2: area_mm2(&plate.openings),
        islands: island_spots,
        thin_webs: web_spots,
        narrow_openings: narrow_spots,
        fits_bed: fits_bed((width_mm, height_mm), settings.bed_mm),
        flat_bridges: plate.flat_bridges.clone(),
    };
    Ok((report, PlateProblems { islands, thin_webs, narrow_openings }))
}

/// A plate as a page draws it: SVG path data in millimetres, y down, to fill with the
/// even-odd rule. The plate path holds all its material, islands included, with the
/// openings as holes; the others lie over it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlateDrawing {
    /// Width of the plate.
    pub width_mm: f64,
    /// Height of the plate.
    pub height_mm: f64,
    /// All of the plate's material.
    pub plate: String,
    /// The pieces that fall out.
    pub islands: String,
    /// Material too narrow to print well.
    pub thin_webs: String,
    /// Openings too narrow to print well.
    pub narrow_openings: String,
}

impl PlateDrawing {
    /// The drawing as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// `plate` and its `problems` as a page draws them.
pub fn plate_drawing(plate: &Plate, problems: &PlateProblems) -> PlateDrawing {
    let height = plate.size_um.1;
    PlateDrawing {
        width_mm: mm(plate.size_um.0),
        height_mm: mm(height),
        plate: svg_path(&plate.solid, height),
        islands: svg_path(&problems.islands, height),
        thin_webs: svg_path(&problems.thin_webs, height),
        narrow_openings: svg_path(&problems.narrow_openings, height),
    }
}

fn spot(shape: &Shape) -> Option<Spot> {
    let shapes = std::slice::from_ref(shape);
    let (lo, hi) = bounds(shapes)?;
    Some(Spot {
        x_mm: mm(lo.x),
        y_mm: mm(lo.y),
        width_mm: mm(hi.x - lo.x),
        height_mm: mm(hi.y - lo.y),
        area_mm2: area_mm2(shapes),
    })
}

/// The parts of `shapes` narrower than `width_um`.
///
/// Shrinking by half the width removes them; growing back restores the rest. With sharp
/// corners, rather than round ones, growing back restores corners exactly, so only narrow
/// parts and corners sharper than 1/32 turn are left over; it is also several times faster,
/// since round corners add many points.
fn narrow_parts(shapes: &Shapes, width_um: i32) -> Result<Shapes, Error> {
    let half = width_um / 2;
    let kept = offset(&offset(shapes, -half, Corners::Sharp)?, half, Corners::Sharp)?;
    let left_over = difference(shapes, &kept);
    offset(&offset(&left_over, -SLIVER_UM, Corners::Sharp)?, SLIVER_UM, Corners::Sharp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::inspect;

    const INK: Rgb = [20, 20, 20];
    const PAPER: Rgb = [250, 250, 250];

    /// A map of `width` by `height` cells of 0.1 mm, ink where `ink` says.
    fn map(width: u32, height: u32, ink: impl Fn(u32, u32) -> bool) -> LabelMap {
        let labels = (0..height).flat_map(|y| (0..width).map(move |x| (x, y))).map(|(x, y)| u8::from(!ink(x, y)));
        LabelMap { width, height, cell_um: 100, palette: vec![INK, PAPER], labels: labels.collect() }
    }

    fn ring(x: u32, y: u32) -> bool {
        let (dx, dy) = (f64::from(x) - 99.5, f64::from(y) - 59.5);
        (30.0 * 30.0..50.0 * 50.0).contains(&(dx * dx + dy * dy))
    }

    #[test]
    fn a_ring_leaves_an_island() {
        let map = map(200, 120, ring);
        let plates = build_plates(&map, &PlateSettings::default(), &[PAPER]).unwrap();
        assert_eq!(plates.len(), 1);
        let plate = &plates[0];
        assert_eq!((plate.paint, plate.size_um), (0, (40_000, 32_000)));
        assert_eq!(plate.file_name(), "plate-01-141414.stl");
        let report = check_plate(plate, &PlateSettings::default()).unwrap();
        assert_eq!((report.width_mm, report.height_mm, report.openings), (40.0, 32.0, 1));
        // The disc inside the ring, 6 mm across, centred on the artwork's centre.
        assert_eq!(report.islands.len(), 1);
        let disc = report.islands[0];
        assert!((disc.x_mm + disc.width_mm / 2.0 - 20.0).abs() < 0.02, "{disc:?}");
        assert!((disc.width_mm - 6.0).abs() < 0.1, "{disc:?}");
        // The traced ring is within a percent of its cells; nothing is narrow; it fits.
        assert!((report.opening_mm2 / report.paint_mm2 - 1.0).abs() < 0.01, "{report:?}");
        assert!(report.thin_webs.is_empty() && report.narrow_openings.is_empty(), "{report:?}");
        assert!(report.fits_bed);

        // The shapes behind the report, and the plate as a page draws it.
        let (_, problems) = inspect_plate(plate, &PlateSettings::default()).unwrap();
        assert_eq!((problems.islands.len(), problems.thin_webs.len()), (1, 0));
        let drawing = plate_drawing(plate, &problems);
        assert_eq!((drawing.width_mm, drawing.height_mm), (40.0, 32.0));
        assert!(drawing.plate.starts_with('M') && drawing.islands.starts_with('M'), "{drawing:?}");
        assert_eq!((drawing.thin_webs.as_str(), drawing.narrow_openings.as_str()), ("", ""));

        // As JSON, with the plate's file name and figures to the thousandth.
        let json = reports_json(&plates, std::slice::from_ref(&report));
        let entry = &json[0];
        assert_eq!(
            (entry["stencil"].as_str(), entry["colour"].as_str()),
            (Some("plate-01-141414.stl"), Some("#141414"))
        );
        assert_eq!((entry["paint"].as_u64(), entry["width_mm"].as_f64()), (Some(0), Some(40.0)));
        let x = entry["islands"][0]["x_mm"].as_f64().unwrap();
        assert!((x - disc.x_mm).abs() < 0.0005 && (x * 1000.0).fract() == 0.0, "{x}");
        assert_eq!(entry["thin_webs"], serde_json::json!([]));
    }

    #[test]
    fn the_mesh_is_the_plate_less_its_openings() {
        let map = map(200, 120, ring);
        let plates = build_plates(&map, &PlateSettings::default(), &[]).unwrap();
        assert_eq!(plates.len(), 2);
        for plate in &plates {
            let report = inspect(&plate.mesh(1.2).unwrap());
            assert!(report.is_closed(), "{report:?}");
            assert_eq!(report.pieces, plate.solid.len());
            let area = 40.0 * 32.0 - area_mm2(&plate.openings);
            assert!((report.volume_mm3 - area * 1.2).abs() < 1e-6, "{} {}", report.volume_mm3, area * 1.2);
            assert_eq!((report.min_mm, report.max_mm), ([0.0; 3], [40.0, 32.0, 1.2]));
        }
        // The paper's plate: the ring itself is cut off by the paper inside and outside it.
        assert_eq!(plates[1].solid.len(), 2);
        let files = plate_files(&plates, 1.2).unwrap();
        assert_eq!(
            files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(),
            ["plate-01-141414.stl", "plate-02-fafafa.stl"]
        );
        assert!(files[1].1.starts_with(b"stencilator plate rgb(250, 250, 250)"));
        // Side by side in one 3MF: two columns, one row.
        let xml = crate::zip::unzip(&plates_3mf(&plates, 1.2).unwrap()).remove(2).1;
        let xml = String::from_utf8(xml).unwrap();
        assert!(xml.contains(r#"<item objectid="2" transform="1 0 0 0 1 0 0 0 1 0 0 0"/>"#));
        assert!(xml.contains(r#"<item objectid="3" transform="1 0 0 0 1 0 0 0 1 50 0 0"/>"#));
    }

    #[test]
    fn flat_bridges_hold_nested_islands_in_one_solid() {
        let nested = map(300, 240, |x, y| {
            let r = x.abs_diff(149).max(y.abs_diff(119));
            (15..35).contains(&r) || (60..100).contains(&r)
        });
        let settings = PlateSettings::default();
        let original = &build_plates(&nested, &settings, &[PAPER]).unwrap()[0];
        assert_eq!(original.solid.len(), 3);
        let settings = PlateSettings { bridge_width_mm: Some(1.2), ..settings };
        let plate = &build_plates(&nested, &settings, &[PAPER]).unwrap()[0];
        let report = check_plate(plate, &settings).unwrap();
        let bridges = report.flat_bridges.unwrap();
        assert!(report.islands.is_empty() && bridges.single_bridge_islands.is_empty(), "{bridges:?}");
        assert_eq!(bridges.count, 4, "{bridges:?}");
        assert!(bridges.covered_mm2 > 0.0, "{bridges:?}");
        assert_eq!(plate.size_um, original.size_um);
        assert!((report.opening_mm2 + bridges.covered_mm2 - area_mm2(&original.openings)).abs() < 1e-6, "{bridges:?}");
        let mesh = inspect(&plate.mesh(settings.thickness_mm).unwrap());
        assert!(mesh.is_closed() && mesh.pieces == 1, "{mesh:?}");
        let area = report.width_mm * report.height_mm - report.opening_mm2;
        assert!((mesh.volume_mm3 - area * settings.thickness_mm).abs() < 1e-6, "{mesh:?}");
    }

    #[test]
    fn a_flat_bridge_needs_material_across_its_full_width() {
        let tiny = map(200, 120, |x, y| {
            (20..180).contains(&x) && (20..100).contains(&y) && !((98..102).contains(&x) && (58..62).contains(&y))
        });
        let settings = PlateSettings { bridge_width_mm: Some(1.2), ..PlateSettings::default() };
        let plate = &build_plates(&tiny, &settings, &[PAPER]).unwrap()[0];
        let report = check_plate(plate, &settings).unwrap();
        assert_eq!(report.flat_bridges.unwrap().count, 0);
        assert_eq!(report.islands.len(), 1);
    }

    #[test]
    fn thin_webs_and_narrow_openings_are_found_and_corners_are_not() {
        // Two square openings 0.3 mm apart, two 0.8 mm apart, and a slit 0.3 mm wide.
        let ink = |x: u32, y: u32| {
            let square = |x0: u32| (x0..x0 + 40).contains(&x) && (20..60).contains(&y);
            square(10) || square(53) || square(101) || ((150..190).contains(&x) && (38..41).contains(&y))
        };
        let plate = &build_plates(&map(200, 80, ink), &PlateSettings::default(), &[PAPER]).unwrap()[0];
        let report = check_plate(plate, &PlateSettings::default()).unwrap();
        assert_eq!(report.thin_webs.len(), 1, "{report:?}");
        let web = report.thin_webs[0];
        // Between x = 5 and 5.3 mm on the artwork, 10 mm in from the plate's edge.
        assert!((web.x_mm + web.width_mm / 2.0 - 15.15).abs() < 0.05, "{web:?}");
        assert!((web.height_mm - 4.0).abs() < 0.3, "{web:?}");
        assert_eq!(report.narrow_openings.len(), 1, "{report:?}");
        assert!((report.narrow_openings[0].width_mm - 4.0).abs() < 0.2, "{report:?}");
        // The shapes are the narrow parts themselves: the web is 0.3 mm wide, not a box.
        let (_, problems) = inspect_plate(plate, &PlateSettings::default()).unwrap();
        assert_eq!((problems.thin_webs.len(), problems.narrow_openings.len()), (1, 1));
        let web_area = area_mm2(&problems.thin_webs);
        assert!(web_area > 0.5 && web_area < 1.5, "{web_area}");
        // Squares that touch at a corner make a point narrower than anything, but a harmless one.
        let corners = |x: u32, y: u32| {
            let square = |x0: u32, y0: u32| (x0..x0 + 40).contains(&x) && (y0..y0 + 40).contains(&y);
            square(10, 10) || square(50, 50) || square(90, 10)
        };
        let touching = &build_plates(&map(140, 100, corners), &PlateSettings::default(), &[]).unwrap();
        for plate in touching {
            let report = check_plate(plate, &PlateSettings::default()).unwrap();
            assert!(report.thin_webs.is_empty() && report.narrow_openings.is_empty(), "{report:?}");
        }
        // With lower limits nothing is reported.
        let lenient = PlateSettings { min_web_mm: 0.2, min_opening_mm: 0.2, ..PlateSettings::default() };
        let report = check_plate(plate, &lenient).unwrap();
        assert!(report.thin_webs.is_empty() && report.narrow_openings.is_empty(), "{report:?}");
    }

    #[test]
    fn large_plates_do_not_fit_the_bed() {
        let map = LabelMap { width: 2400, height: 1000, cell_um: 100, palette: vec![INK], labels: vec![0; 2_400_000] };
        let settings = PlateSettings::default();
        let plate = &build_plates(&map, &settings, &[]).unwrap()[0];
        // 260 by 120 mm with the margins.
        assert!(!check_plate(plate, &settings).unwrap().fits_bed);
        assert!(check_plate(plate, &PlateSettings { bed_mm: (150.0, 300.0), ..settings }).unwrap().fits_bed);
    }

    #[test]
    fn mistakes_are_explained() {
        let map = map(10, 10, |x, _| x < 5);
        assert_eq!(build_plates(&map, &PlateSettings::default(), &[INK, PAPER]).unwrap(), Vec::<Plate>::new());
        let message =
            |settings: PlateSettings, skip: &[Rgb]| build_plates(&map, &settings, skip).unwrap_err().to_string();
        let narrow = PlateSettings { margin_mm: 1.0, ..PlateSettings::default() };
        assert_eq!(message(narrow, &[INK, PAPER]), "The margin must be between 2 and 100 mm.");
        let thin = PlateSettings { thickness_mm: 0.1, ..PlateSettings::default() };
        assert_eq!(message(thin, &[]), "The stencil thickness must be between 0.2 and 20 mm.");
    }
}
