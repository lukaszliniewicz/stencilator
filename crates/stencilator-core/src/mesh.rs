//! Solids from stacks of slabs, for free-form plates.
//!
//! A plate is a stack of slabs, each a set of shapes between two heights: the plate with its
//! openings, and later chamfer steps, connector feet and beams, marks. The mesh is
//!
//! - walls: every contour edge of every slab, extruded over that slab's height;
//! - caps: at each level, the part of the slab below that the slab above does not cover
//!   faces up, and the part of the slab above that the slab below does not cover faces
//!   down;
//! - welded at identical integer vertices, with T-junctions repaired: where a vertex of one
//!   face lies on the edge of another, that edge is split.
//!
//! Outlines that should coincide between neighbouring slabs, such as the plate's edge, can
//! come out of the polygon engine a micrometre or two apart, since it rounds every crossing
//! to whole micrometres. That leaves slivers and knife edges, so the slabs are first aligned
//! with each other: corners within [`ALIGN_UM`] of each other become one, and a corner that
//! close to an edge is added to the edge.
//!
//! Only plane booleans are needed, never solid ones, and every coordinate is an integer
//! micrometre, so the result is the same to the bit natively and in the browser.

use std::collections::{BTreeMap, BTreeSet};

use crate::Error;
use crate::geometry::{Contour, Point, Shapes, UM_PER_MM, clean, difference};
use crate::stencil::Triangle;

/// Shapes between two heights, in micrometres.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slab {
    /// Bottom.
    pub z0: i32,
    /// Top.
    pub z1: i32,
    /// What the slab covers.
    pub shapes: Shapes,
}

/// An indexed triangle mesh in micrometres, each triangle counter-clockwise seen from
/// outside.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mesh {
    /// Corners.
    pub vertices: Vec<[i32; 3]>,
    /// Three corner indices each.
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// The triangles in millimetres, as [`crate::stencil::encode_stl`] takes them.
    pub fn triangles_mm(&self) -> Vec<Triangle> {
        self.triangles.iter().map(|t| t.map(|i| self.vertex(i).map(|c| f64::from(c) / UM_PER_MM))).collect()
    }

    fn vertex(&self, index: u32) -> [i32; 3] {
        self.vertices[index as usize]
    }
}

type Corners = [[i32; 3]; 3];

/// Largest distance, in micrometres, at which a vertex counts as lying on an edge. Points
/// where edges cross are snapped to the grid and can sit one unit off the edge.
const ON_EDGE_TOLERANCE: i128 = 1;

/// Largest distance, in micrometres, by which outlines are moved to meet each other. Far
/// below what any printer resolves, and above the polygon engine's rounding.
pub const ALIGN_UM: i32 = 3;

/// Meshes a stack of slabs into a closed solid.
///
/// # Errors
///
/// [`Error::Geometry`] when there are no slabs, when the slabs leave gaps or have no
/// height, or when the surface cannot be closed, which would be a fault in the engine.
pub fn mesh_slabs(slabs: &[Slab]) -> Result<Mesh, Error> {
    let Some(top) = slabs.last() else { return Err(fault("the plate has no layers")) };
    if slabs.windows(2).any(|pair| pair[0].z1 != pair[1].z0) || slabs.iter().any(|slab| slab.z1 <= slab.z0) {
        return Err(fault("the layers of the plate do not stack"));
    }
    let mut cleaned: Vec<Shapes> = slabs.iter().map(|slab| clean(&slab.shapes)).collect();
    if cleaned.len() > 1 {
        cleaned = align(&cleaned);
    }
    let empty: Shapes = Vec::new();
    let mut corners: Vec<Corners> = Vec::new();
    for (slab, shapes) in slabs.iter().zip(&cleaned) {
        walls(shapes, slab.z0, slab.z1, &mut corners);
    }
    for level in 0..=slabs.len() {
        let below = level.checked_sub(1).map_or(&empty, |i| &cleaned[i]);
        let above = cleaned.get(level).unwrap_or(&empty);
        let z = slabs.get(level).map_or(top.z1, |slab| slab.z0);
        cap(&difference(below, above), z, true, &mut corners)?;
        cap(&difference(above, below), z, false, &mut corners)?;
    }
    let mut mesh = weld(&corners)?;
    repair_t_junctions(&mut mesh)?;
    Ok(mesh)
}

/// The slabs' shapes with every corner within [`ALIGN_UM`] of another merged into one, and
/// every corner that close to the inside of an edge added to that edge, across all slabs, so
/// that outlines meant to meet do so exactly.
fn align(slabs: &[Shapes]) -> Vec<Shapes> {
    let mut corners: Vec<Point> = slabs.iter().flatten().flatten().flatten().copied().collect();
    corners.sort_unstable_by_key(|p| (p.x, p.y));
    corners.dedup();
    // Each corner goes to the first kept corner near it, in order of x; kept corners stay
    // sorted by x because they are taken in that order.
    let tolerance2 = i64::from(ALIGN_UM) * i64::from(ALIGN_UM);
    let mut kept: Vec<Point> = Vec::new();
    let mut merged: BTreeMap<Point, Point> = BTreeMap::new();
    for &c in &corners {
        let near = corners_between(&kept, c.x - ALIGN_UM, c.x).iter().copied().find(|&k| distance2(k, c) <= tolerance2);
        match near {
            Some(k) => {
                merged.insert(c, k);
            }
            None => kept.push(c),
        }
    }
    kept.sort_unstable_by_key(|p| (p.x, p.y));
    slabs
        .iter()
        .map(|shapes| {
            let aligned: Shapes = shapes
                .iter()
                .map(|shape| shape.iter().map(|contour| align_contour(contour, &merged, &kept)).collect())
                .collect();
            clean(&aligned)
        })
        .collect()
}

/// Corners whose x lies within `x0..=x1`, from corners sorted by x.
fn corners_between(corners: &[Point], x0: i32, x1: i32) -> &[Point] {
    let start = corners.partition_point(|p| p.x < x0);
    let end = corners.partition_point(|p| p.x <= x1);
    &corners[start..end.max(start)]
}

fn align_contour(contour: &Contour, merged: &BTreeMap<Point, Point>, kept: &[Point]) -> Contour {
    let moved: Vec<Point> = contour.iter().map(|p| merged.get(p).copied().unwrap_or(*p)).collect();
    let mut out: Contour = Vec::with_capacity(moved.len());
    for (i, &a) in moved.iter().enumerate() {
        let b = moved[(i + 1) % moved.len()];
        out.push(a);
        let mut on: Vec<(i128, Point)> = corners_between(kept, a.x.min(b.x) - ALIGN_UM, a.x.max(b.x) + ALIGN_UM)
            .iter()
            .filter(|c| c.y >= a.y.min(b.y) - ALIGN_UM && c.y <= a.y.max(b.y) + ALIGN_UM)
            .filter_map(|&c| near_edge(a, b, c).map(|along| (along, c)))
            .collect();
        on.sort_unstable_by_key(|&(along, c)| (along, c.x, c.y));
        out.extend(on.into_iter().map(|(_, c)| c));
    }
    out.dedup();
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

fn distance2(a: Point, b: Point) -> i64 {
    let (dx, dy) = (i64::from(a.x) - i64::from(b.x), i64::from(a.y) - i64::from(b.y));
    dx * dx + dy * dy
}

/// Where `c` lies within [`ALIGN_UM`] of the inside of edge `a`-`b`, how far along it (in
/// units of its squared length); otherwise `None`.
fn near_edge(a: Point, b: Point, c: Point) -> Option<i128> {
    if c == a || c == b {
        return None;
    }
    let (ex, ey) = (i128::from(b.x) - i128::from(a.x), i128::from(b.y) - i128::from(a.y));
    let (tx, ty) = (i128::from(c.x) - i128::from(a.x), i128::from(c.y) - i128::from(a.y));
    let along = ex * tx + ey * ty;
    let length2 = ex * ex + ey * ey;
    if along <= 0 || along >= length2 {
        return None;
    }
    let off = ex * ty - ey * tx;
    let tolerance = i128::from(ALIGN_UM);
    (off * off <= tolerance * tolerance * length2).then_some(along)
}

fn fault(what: &str) -> Error {
    Error::Geometry(format!("The plate could not be built: {what}. This is a fault in Stencilator; please report it."))
}

fn walls(shapes: &Shapes, z0: i32, z1: i32, out: &mut Vec<Corners>) {
    for contour in shapes.iter().flatten() {
        for (i, a) in contour.iter().enumerate() {
            let b = contour[(i + 1) % contour.len()];
            let (a0, b0) = ([a.x, a.y, z0], [b.x, b.y, z0]);
            let (a1, b1) = ([a.x, a.y, z1], [b.x, b.y, z1]);
            out.push([a0, b0, b1]);
            out.push([a0, b1, a1]);
        }
    }
}

fn cap(shapes: &Shapes, z: i32, facing_up: bool, out: &mut Vec<Corners>) -> Result<(), Error> {
    for shape in shapes {
        let mut coords = Vec::new();
        let mut holes = Vec::new();
        let mut points = Vec::new();
        for (i, contour) in shape.iter().enumerate() {
            if i > 0 {
                holes.push(points.len());
            }
            for p in contour {
                coords.extend([f64::from(p.x), f64::from(p.y)]);
                points.push(*p);
            }
        }
        let indices = earcutr::earcut(&coords, &holes, 2).map_err(|_| fault("a face could not be triangulated"))?;
        for t in indices.as_chunks::<3>().0 {
            let (a, b, c) = (points[t[0]], points[t[1]], points[t[2]]);
            let area = cross(a, b, c);
            if area == 0 {
                continue; // in a straight line: the T-junction repair closes the gap
            }
            let ordered = if (area > 0) == facing_up { [a, b, c] } else { [a, c, b] };
            out.push(ordered.map(|p| [p.x, p.y, z]));
        }
    }
    Ok(())
}

fn cross(a: Point, b: Point, c: Point) -> i64 {
    let (abx, aby) = (i64::from(b.x) - i64::from(a.x), i64::from(b.y) - i64::from(a.y));
    let (acx, acy) = (i64::from(c.x) - i64::from(a.x), i64::from(c.y) - i64::from(a.y));
    abx * acy - aby * acx
}

fn weld(corners: &[Corners]) -> Result<Mesh, Error> {
    let mut index: BTreeMap<[i32; 3], u32> = BTreeMap::new();
    let mut mesh = Mesh::default();
    for triangle in corners {
        let mut ids = [0_u32; 3];
        for (id, vertex) in ids.iter_mut().zip(triangle) {
            *id = if let Some(&existing) = index.get(vertex) {
                existing
            } else {
                let new = u32::try_from(mesh.vertices.len()).map_err(|_| too_detailed())?;
                mesh.vertices.push(*vertex);
                index.insert(*vertex, new);
                new
            };
        }
        if ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2] {
            mesh.triangles.push(ids);
        }
    }
    Ok(mesh)
}

fn too_detailed() -> Error {
    Error::Geometry("The plate is too detailed to build. Try a larger cell size.".to_owned())
}

fn directed_edges(triangles: &[[u32; 3]]) -> BTreeMap<(u32, u32), u32> {
    let mut edges = BTreeMap::new();
    for t in triangles {
        for i in 0..3 {
            *edges.entry((t[i], t[(i + 1) % 3])).or_insert(0) += 1;
        }
    }
    edges
}

/// Where `v` lies strictly inside segment `a`-`b`, within the tolerance, how far along it
/// (in units of the segment's squared length); otherwise `None`.
fn position_on_edge(start: [i32; 3], end: [i32; 3], vertex: [i32; 3]) -> Option<i128> {
    let edge = [0, 1, 2].map(|i| i128::from(end[i]) - i128::from(start[i]));
    let to = [0, 1, 2].map(|i| i128::from(vertex[i]) - i128::from(start[i]));
    let along = edge[0] * to[0] + edge[1] * to[1] + edge[2] * to[2];
    let length2 = edge[0] * edge[0] + edge[1] * edge[1] + edge[2] * edge[2];
    if along <= 0 || along >= length2 {
        return None;
    }
    let off = [edge[1] * to[2] - edge[2] * to[1], edge[2] * to[0] - edge[0] * to[2], edge[0] * to[1] - edge[1] * to[0]];
    let off2 = off[0] * off[0] + off[1] * off[1] + off[2] * off[2];
    (off2 <= ON_EDGE_TOLERANCE * ON_EDGE_TOLERANCE * length2).then_some(along)
}

/// Splits edges without a partner at the vertices that lie on them, until every edge has a
/// partner.
fn repair_t_junctions(mesh: &mut Mesh) -> Result<(), Error> {
    loop {
        let edges = directed_edges(&mesh.triangles);
        let unmatched: Vec<(u32, u32)> =
            edges.keys().filter(|&&(a, b)| !edges.contains_key(&(b, a))).copied().collect();
        if unmatched.is_empty() {
            return Ok(());
        }
        let candidates: BTreeSet<u32> = unmatched.iter().flat_map(|&(a, b)| [a, b]).collect();
        let mut changed = false;
        let mut repaired = Vec::with_capacity(mesh.triangles.len());
        for t in &mesh.triangles {
            let mut split = None;
            for i in 0..3 {
                let (a, b, apex) = (t[i], t[(i + 1) % 3], t[(i + 2) % 3]);
                if edges.contains_key(&(b, a)) {
                    continue;
                }
                let (va, vb) = (mesh.vertex(a), mesh.vertex(b));
                let mut on: Vec<(i128, u32)> = candidates
                    .iter()
                    .filter_map(|&v| position_on_edge(va, vb, mesh.vertex(v)).map(|s| (s, v)))
                    .collect();
                if !on.is_empty() {
                    on.sort_unstable();
                    let mut chain = vec![a];
                    chain.extend(on.iter().map(|&(_, v)| v));
                    chain.push(b);
                    split = Some((chain, apex));
                    break;
                }
            }
            match split {
                Some((chain, apex)) => {
                    changed = true;
                    repaired.extend(chain.windows(2).map(|w| [w[0], w[1], apex]));
                }
                None => repaired.push(*t),
            }
        }
        mesh.triangles = repaired;
        if !changed {
            return Err(fault("its surface has gaps"));
        }
    }
}

/// What [`inspect`] found in a mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshReport {
    /// Corners.
    pub vertices: usize,
    /// Triangles.
    pub triangles: usize,
    /// Directed edges with no reversed partner: holes in the surface.
    pub open_edges: usize,
    /// Directed edges used more than once: faces that touch wrongly or are flipped.
    pub repeated_edges: usize,
    /// Triangles with no area.
    pub degenerate_triangles: usize,
    /// Separate pieces, such as islands that are not held.
    pub pieces: usize,
    /// Vertices less edges plus faces, which tells how many holes run through the solid.
    pub euler: i64,
    /// Enclosed volume.
    pub volume_mm3: f64,
    /// Smallest corner of the bounding box.
    pub min_mm: [f64; 3],
    /// Largest corner of the bounding box.
    pub max_mm: [f64; 3],
}

impl MeshReport {
    /// Closed and consistently oriented, every piece a solid of positive volume, as a
    /// slicer needs it.
    pub fn is_closed(&self) -> bool {
        self.open_edges == 0 && self.repeated_edges == 0 && self.degenerate_triangles == 0 && self.volume_mm3 > 0.0
    }
}

/// Checks a mesh as a solid.
pub fn inspect(mesh: &Mesh) -> MeshReport {
    let edges = directed_edges(&mesh.triangles);
    let open_edges = edges.keys().filter(|&&(a, b)| !edges.contains_key(&(b, a))).count();
    let repeated_edges = edges.values().filter(|&&n| n > 1).count();
    let undirected: BTreeSet<(u32, u32)> = edges.keys().map(|&(a, b)| (a.min(b), a.max(b))).collect();

    let vertex = |i: u32| mesh.vertex(i).map(i128::from);
    let mut volume6: i128 = 0;
    let mut degenerate_triangles = 0;
    for t in &mesh.triangles {
        let [p0, p1, p2] = t.map(vertex);
        volume6 += p0[0] * (p1[1] * p2[2] - p1[2] * p2[1]) - p0[1] * (p1[0] * p2[2] - p1[2] * p2[0])
            + p0[2] * (p1[0] * p2[1] - p1[1] * p2[0]);
        let (e1, e2) = ([0, 1, 2].map(|i| p1[i] - p0[i]), [0, 1, 2].map(|i| p2[i] - p0[i]));
        if [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]] == [0, 0, 0] {
            degenerate_triangles += 1;
        }
    }

    let mut min_mm = [f64::INFINITY; 3];
    let mut max_mm = [f64::NEG_INFINITY; 3];
    for v in &mesh.vertices {
        for i in 0..3 {
            let mm = f64::from(v[i]) / UM_PER_MM;
            min_mm[i] = min_mm[i].min(mm);
            max_mm[i] = max_mm[i].max(mm);
        }
    }
    let count = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    MeshReport {
        vertices: mesh.vertices.len(),
        triangles: mesh.triangles.len(),
        open_edges,
        repeated_edges,
        degenerate_triangles,
        pieces: pieces(&mesh.triangles),
        euler: count(mesh.vertices.len()) - count(undirected.len()) + count(mesh.triangles.len()),
        volume_mm3: volume_mm3(volume6),
        min_mm,
        max_mm,
    }
}

#[allow(clippy::cast_precision_loss, reason = "a plate's volume in cubic micrometres needs no more than 53 bits")]
fn volume_mm3(volume6: i128) -> f64 {
    volume6 as f64 / 6.0 / (UM_PER_MM * UM_PER_MM * UM_PER_MM)
}

/// Pieces joined through shared edges, counted with a union-find over the triangles.
fn pieces(triangles: &[[u32; 3]]) -> usize {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..triangles.len()).collect();
    let mut first_owner: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for (ti, t) in triangles.iter().enumerate() {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            if let Some(&other) = first_owner.get(&(a.min(b), a.max(b))) {
                let (ra, rb) = (root(&mut parent, ti), root(&mut parent, other));
                parent[ra] = rb;
            } else {
                first_owner.insert((a.min(b), a.max(b)), ti);
            }
        }
    }
    (0..parent.len()).filter(|&i| root(&mut parent, i) == i).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Contour, Corners, area_mm2, offset, rectangle};

    fn reversed(contour: Contour) -> Contour {
        contour.into_iter().rev().collect()
    }

    #[test]
    fn a_plate_with_a_hole_is_a_closed_ring() {
        let ring = vec![vec![rectangle(0, 0, 10_000, 10_000), reversed(rectangle(3000, 3000, 7000, 7000))]];
        let report = inspect(&mesh_slabs(&[Slab { z0: 0, z1: 1000, shapes: ring }]).unwrap());
        assert!(report.is_closed(), "{report:?}");
        assert_eq!((report.pieces, report.euler), (1, 0));
        assert!((report.volume_mm3 - 84.0).abs() < 1e-9);
        assert_eq!(report.max_mm, [10.0, 10.0, 1.0]);
    }

    #[test]
    fn an_island_is_a_second_piece() {
        let plate = vec![
            vec![rectangle(0, 0, 10_000, 10_000), reversed(rectangle(2000, 2000, 8000, 8000))],
            vec![rectangle(4000, 4000, 6000, 6000)],
        ];
        let report = inspect(&mesh_slabs(&[Slab { z0: 0, z1: 1200, shapes: plate }]).unwrap());
        assert!(report.is_closed(), "{report:?}");
        assert_eq!(report.pieces, 2);
    }

    /// A 60 by 40 mm plate whose ring-shaped opening holds a disc, carried by three raised
    /// bridges over a chamfered opening: the Phase 0 spike, rotated so that slab edges cross
    /// off the integer grid.
    fn bridged_plate(rotation_deg: f64, corners: Corners) -> Vec<Slab> {
        let (sin, cos) = libm::sincos(rotation_deg.to_radians());
        #[allow(clippy::cast_possible_truncation, reason = "millimetres within a 60 mm plate")]
        let point = |x: f64, y: f64| {
            let (dx, dy) = (x - 30.0, y - 20.0);
            Point::new(
                libm::round((30.0 + dx * cos - dy * sin) * 1000.0) as i32,
                libm::round((20.0 + dx * sin + dy * cos) * 1000.0) as i32,
            )
        };
        let circle = |radius: f64, clockwise: bool| -> Contour {
            let mut points: Contour = (0..96)
                .map(|i| {
                    let (s, c) = libm::sincos(std::f64::consts::TAU * f64::from(i) / 96.0);
                    point(30.0 + radius * c, 20.0 + radius * s)
                })
                .collect();
            if clockwise {
                points.reverse();
            }
            points
        };
        let rect =
            |x0: f64, y0: f64, x1: f64, y1: f64| vec![point(x0, y0), point(x1, y0), point(x1, y1), point(x0, y1)];
        let bar = |angle: f64, r0: f64, r1: f64, width: f64| {
            let (uy, ux) = libm::sincos(angle.to_radians());
            let (nx, ny) = (-uy * width / 2.0, ux * width / 2.0);
            let at = |r: f64, side: f64| point(30.0 + ux * r + nx * side, 20.0 + uy * r + ny * side);
            vec![at(r0, -1.0), at(r1, -1.0), at(r1, 1.0), at(r0, 1.0)]
        };
        let plate: Shapes = vec![vec![rect(0.0, 0.0, 60.0, 40.0)]];
        let openings: Shapes = vec![vec![circle(12.0, false), circle(6.0, true)], vec![rect(5.0, 0.2, 10.0, 3.0)]];
        let chamfer = offset(&openings, 300, corners).unwrap();
        let (mut feet, mut beams): (Shapes, Shapes) = (Vec::new(), Vec::new());
        for angle in [30.0, 150.0, 270.0] {
            feet.push(vec![bar(angle, 3.8, 5.0, 1.0)]);
            feet.push(vec![bar(angle, 12.9, 14.2, 1.0)]);
            beams.push(vec![bar(angle, 3.5, 14.5, 1.2)]);
        }
        vec![
            Slab { z0: 0, z1: 600, shapes: difference(&plate, &openings) },
            Slab { z0: 600, z1: 1200, shapes: difference(&plate, &chamfer) },
            Slab { z0: 1200, z1: 4000, shapes: feet },
            Slab { z0: 4000, z1: 5000, shapes: beams },
        ]
    }

    #[test]
    fn a_bridged_island_makes_one_closed_solid_at_any_rotation() {
        // Phase 0 tried eight rotations and all passed; a sweep of 3,600 found 7 to 12 per
        // cent failing until the slabs were aligned. These include angles that failed.
        for corners in [Corners::Bevel, Corners::Round] {
            for rotation in [0.0, 7.0, 16.5, 26.3, 27.5, 45.0, 121.7, 200.0, 333.3] {
                let report = inspect(&mesh_slabs(&bridged_plate(rotation, corners)).unwrap());
                assert!(report.is_closed() && report.pieces == 1, "{corners:?} {rotation}: {report:?}");
                // A ring opening crossed by three bridges: genus 4.
                assert_eq!(report.euler, -6, "{corners:?} {rotation}");
                assert!((report.volume_mm3 - 2494.0).abs() < 5.0, "{corners:?} {rotation}: {}", report.volume_mm3);
            }
        }
    }

    #[test]
    fn outlines_a_few_micrometres_apart_are_made_to_meet() {
        // The upper slab has a corner 2 µm from the lower slab's corner, and another 2 µm
        // above the lower slab's bottom edge.
        let below = vec![vec![rectangle(0, 0, 10_000, 10_000)]];
        let above =
            vec![vec![vec![Point::new(2, 2), Point::new(5000, 2), Point::new(5000, 5000), Point::new(0, 5000)]]];
        let aligned = align(&[below.clone(), above]);
        // The near corners become one; the corner near the edge joins the edge, so both
        // outlines pass through the very same point.
        assert!(aligned[1][0][0].contains(&Point::new(0, 0)), "{:?}", aligned[1]);
        assert!(!aligned[1][0][0].contains(&Point::new(2, 2)));
        assert!(aligned[0][0][0].contains(&Point::new(5000, 2)), "{:?}", aligned[0]);
        // Moving by 2 µm changes the area by no more than a sliver.
        assert!((area_mm2(&aligned[0]) - area_mm2(&below)).abs() < 0.02);
    }

    #[test]
    fn slabs_must_stack() {
        assert!(matches!(mesh_slabs(&[]), Err(Error::Geometry(_))));
        let square = vec![vec![rectangle(0, 0, 1000, 1000)]];
        let gap = [Slab { z0: 0, z1: 100, shapes: square.clone() }, Slab { z0: 200, z1: 300, shapes: square.clone() }];
        assert!(matches!(mesh_slabs(&gap), Err(Error::Geometry(_))));
        assert!(mesh_slabs(&[Slab { z0: 100, z1: 100, shapes: square }]).is_err());
    }

    #[test]
    fn millimetres_for_stl() {
        let square = vec![vec![rectangle(0, 0, 2500, 1000)]];
        let mesh = mesh_slabs(&[Slab { z0: 0, z1: 500, shapes: square }]).unwrap();
        let mm = mesh.triangles_mm();
        assert_eq!(mm.len(), 12);
        assert!(mm.iter().flatten().all(|p| [0.0, 2.5].contains(&p[0]) && [0.0, 0.5].contains(&p[2])));
    }
}
