//! The slab mesher (spike 2).
//!
//! A plate is a stack of slabs. Each slab is a set of polygons between two
//! heights. The mesh is:
//!
//! - walls: every polygon edge of every slab, extruded over that slab's height;
//! - caps: at each level, the part of the slab below that the slab above does
//!   not cover faces up, and the part of the slab above that the slab below
//!   does not cover faces down;
//! - welding of identical integer vertices, then repair of T-junctions: where a
//!   vertex of one face lies on the edge of another, that edge is split.
//!
//! Only 2D booleans are needed, never 3D ones.

use std::collections::{BTreeMap, BTreeSet};

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::Overlay;
pub use i_overlay::core::overlay_rule::OverlayRule as Rule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::i_float::int::point::IntPoint;
use i_overlay::i_shape::int::shape::IntShapes;

/// A point in micrometres.
pub type Pt = IntPoint<i32>;
/// Shapes: each is an outer contour (counter-clockwise) and its holes (clockwise).
pub type Shapes = IntShapes<i32>;

/// Polygons between two heights (micrometres).
#[derive(Debug, Clone)]
pub struct Slab {
    pub z0: i32,
    pub z1: i32,
    pub shapes: Shapes,
}

/// An indexed triangle mesh in micrometres, outward-facing counter-clockwise.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<[i32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

type Triangle = [[i32; 3]; 3];

/// Largest distance at which a vertex counts as lying on an edge (micrometres).
/// Snapped intersection points can sit up to one unit off the original edge.
const ON_EDGE_TOLERANCE: i128 = 1;

/// A boolean operation with the non-zero fill rule.
#[must_use]
pub fn boolean(subject: &Shapes, clip: &Shapes, rule: OverlayRule) -> Shapes {
    if subject.is_empty() && clip.is_empty() {
        return Vec::new();
    }
    Overlay::from_subj_and_clip(subject, clip).overlay(rule, FillRule::NonZero)
}

/// Cleans shapes: merges overlaps, orients contours, drops collinear points.
#[must_use]
pub fn clean(shapes: &Shapes) -> Shapes {
    boolean(shapes, &Vec::new(), OverlayRule::Subject)
}

/// Builds and repairs the mesh of a stack of slabs.
pub fn mesh_slabs(slabs: &[Slab]) -> Result<Mesh, String> {
    for pair in slabs.windows(2) {
        if pair[0].z1 != pair[1].z0 {
            return Err("slabs must be stacked without gaps".to_owned());
        }
    }
    if slabs.iter().any(|slab| slab.z1 <= slab.z0) {
        return Err("every slab needs a positive height".to_owned());
    }
    let cleaned: Vec<Shapes> = slabs.iter().map(|slab| clean(&slab.shapes)).collect();
    let empty: Shapes = Vec::new();
    let mut triangles: Vec<Triangle> = Vec::new();
    for (slab, shapes) in slabs.iter().zip(&cleaned) {
        walls(shapes, slab.z0, slab.z1, &mut triangles);
    }
    for level in 0..=slabs.len() {
        let below = if level == 0 { &empty } else { &cleaned[level - 1] };
        let above = cleaned.get(level).unwrap_or(&empty);
        let z = slabs.get(level).map_or_else(|| slabs[level - 1].z1, |slab| slab.z0);
        cap(&boolean(below, above, OverlayRule::Difference), z, true, &mut triangles)?;
        cap(&boolean(above, below, OverlayRule::Difference), z, false, &mut triangles)?;
    }
    let mut mesh = weld(&triangles);
    repair_t_junctions(&mut mesh)?;
    Ok(mesh)
}

fn walls(shapes: &Shapes, z0: i32, z1: i32, out: &mut Vec<Triangle>) {
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

fn cap(shapes: &Shapes, z: i32, facing_up: bool, out: &mut Vec<Triangle>) -> Result<(), String> {
    for shape in shapes {
        let mut coords = Vec::new();
        let mut holes = Vec::new();
        let mut points = Vec::new();
        for (i, contour) in shape.iter().enumerate() {
            if i > 0 {
                holes.push(points.len());
            }
            for p in contour {
                coords.push(f64::from(p.x));
                coords.push(f64::from(p.y));
                points.push(*p);
            }
        }
        let indices =
            earcutr::earcut(&coords, &holes, 2).map_err(|error| format!("triangulation failed: {error:?}"))?;
        for t in indices.as_chunks::<3>().0 {
            let (a, b, c) = (points[t[0]], points[t[1]], points[t[2]]);
            let area = cross2(a, b, c);
            if area == 0 {
                continue; // collinear; the T-junction repair closes the gap
            }
            let ordered = if (area > 0) == facing_up { [a, b, c] } else { [a, c, b] };
            out.push(ordered.map(|p| [p.x, p.y, z]));
        }
    }
    Ok(())
}

fn cross2(a: Pt, b: Pt, c: Pt) -> i64 {
    let (abx, aby) = (i64::from(b.x) - i64::from(a.x), i64::from(b.y) - i64::from(a.y));
    let (acx, acy) = (i64::from(c.x) - i64::from(a.x), i64::from(c.y) - i64::from(a.y));
    abx * acy - aby * acx
}

fn weld(triangles: &[Triangle]) -> Mesh {
    let mut index: BTreeMap<[i32; 3], u32> = BTreeMap::new();
    let mut mesh = Mesh::default();
    for triangle in triangles {
        let mut ids = [0_u32; 3];
        for (id, vertex) in ids.iter_mut().zip(triangle) {
            *id = *index.entry(*vertex).or_insert_with(|| {
                mesh.vertices.push(*vertex);
                (mesh.vertices.len() - 1) as u32
            });
        }
        if ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2] {
            mesh.triangles.push(ids);
        }
    }
    mesh
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

/// Where `v` lies strictly inside segment `a`-`b` (within the tolerance), its
/// position along the segment; otherwise `None`.
fn position_on_edge(a: [i32; 3], b: [i32; 3], v: [i32; 3]) -> Option<i128> {
    let d = [0, 1, 2].map(|i| i128::from(b[i]) - i128::from(a[i]));
    let w = [0, 1, 2].map(|i| i128::from(v[i]) - i128::from(a[i]));
    let along = d[0] * w[0] + d[1] * w[1] + d[2] * w[2];
    let length2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if along <= 0 || along >= length2 {
        return None;
    }
    let cross = [d[1] * w[2] - d[2] * w[1], d[2] * w[0] - d[0] * w[2], d[0] * w[1] - d[1] * w[0]];
    let cross2 = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
    (cross2 <= ON_EDGE_TOLERANCE * ON_EDGE_TOLERANCE * length2).then_some(along)
}

/// Splits unmatched edges at vertices that lie on them, until every edge has a
/// partner or nothing more can be split. Returns the number of splits.
fn repair_t_junctions(mesh: &mut Mesh) -> Result<usize, String> {
    let mut splits = 0;
    loop {
        let edges = directed_edges(&mesh.triangles);
        let unmatched: Vec<(u32, u32)> =
            edges.keys().filter(|&&(a, b)| !edges.contains_key(&(b, a))).copied().collect();
        if unmatched.is_empty() {
            return Ok(splits);
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
                let (va, vb) = (mesh.vertices[a as usize], mesh.vertices[b as usize]);
                let mut on: Vec<(i128, u32)> = candidates
                    .iter()
                    .filter_map(|&v| position_on_edge(va, vb, mesh.vertices[v as usize]).map(|s| (s, v)))
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
                    splits += chain.len() - 2;
                    changed = true;
                    repaired.extend(chain.windows(2).map(|w| [w[0], w[1], apex]));
                }
                None => repaired.push(*t),
            }
        }
        mesh.triangles = repaired;
        if !changed {
            return Err(format!("{} edges have no partner and cannot be split", unmatched.len()));
        }
    }
}

/// What a mesh check found.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub vertices: usize,
    pub triangles: usize,
    /// Directed edges with no reversed partner (holes in the surface).
    pub open_edges: usize,
    /// Directed edges used more than once (non-manifold or flipped faces).
    pub repeated_edges: usize,
    pub degenerate_triangles: usize,
    /// Pieces connected through shared edges.
    pub components: usize,
    /// V - E + F.
    pub euler: i64,
    pub volume_mm3: f64,
    pub min_mm: [f64; 3],
    pub max_mm: [f64; 3],
}

impl Report {
    /// One closed, consistently oriented solid of positive volume.
    #[must_use]
    pub fn is_single_closed_solid(&self) -> bool {
        self.open_edges == 0
            && self.repeated_edges == 0
            && self.degenerate_triangles == 0
            && self.components == 1
            && self.volume_mm3 > 0.0
    }
}

/// Checks the mesh as a solid.
#[must_use]
pub fn check(mesh: &Mesh) -> Report {
    let edges = directed_edges(&mesh.triangles);
    let open_edges = edges.keys().filter(|&&(a, b)| !edges.contains_key(&(b, a))).count();
    let repeated_edges = edges.values().filter(|&&n| n > 1).count();
    let undirected: BTreeSet<(u32, u32)> = edges.keys().map(|&(a, b)| (a.min(b), a.max(b))).collect();

    let vertex = |i: u32| mesh.vertices[i as usize].map(i128::from);
    let mut volume6: i128 = 0;
    let mut degenerate_triangles = 0;
    for t in &mesh.triangles {
        let (a, b, c) = (vertex(t[0]), vertex(t[1]), vertex(t[2]));
        volume6 += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
        let (u, v) = ([0, 1, 2].map(|i| b[i] - a[i]), [0, 1, 2].map(|i| c[i] - a[i]));
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        if n == [0, 0, 0] {
            degenerate_triangles += 1;
        }
    }

    let mut parent: Vec<usize> = (0..mesh.triangles.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut first_owner: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for (ti, t) in mesh.triangles.iter().enumerate() {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            let key = (a.min(b), a.max(b));
            if let Some(&other) = first_owner.get(&key) {
                let (ra, rb) = (find(&mut parent, ti), find(&mut parent, other));
                parent[ra] = rb;
            } else {
                first_owner.insert(key, ti);
            }
        }
    }
    let components = (0..parent.len()).filter(|&i| find(&mut parent, i) == i).count();

    let mut min_mm = [f64::INFINITY; 3];
    let mut max_mm = [f64::NEG_INFINITY; 3];
    for v in &mesh.vertices {
        for i in 0..3 {
            let mm = f64::from(v[i]) / 1000.0;
            min_mm[i] = min_mm[i].min(mm);
            max_mm[i] = max_mm[i].max(mm);
        }
    }
    Report {
        vertices: mesh.vertices.len(),
        triangles: mesh.triangles.len(),
        open_edges,
        repeated_edges,
        degenerate_triangles,
        components,
        euler: mesh.vertices.len() as i64 - undirected.len() as i64 + mesh.triangles.len() as i64,
        volume_mm3: volume6 as f64 / 6.0 / 1e9,
        min_mm,
        max_mm,
    }
}

/// Binary STL in millimetres.
#[must_use]
pub fn to_stl(mesh: &Mesh, header: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + mesh.triangles.len() * 50);
    let mut head = [0_u8; 80];
    for (slot, byte) in head.iter_mut().zip(header.bytes()) {
        *slot = byte;
    }
    out.extend_from_slice(&head);
    out.extend_from_slice(&(mesh.triangles.len() as u32).to_le_bytes());
    for t in &mesh.triangles {
        let p = t.map(|i| mesh.vertices[i as usize].map(|c| f64::from(c) / 1000.0));
        let u = [0, 1, 2].map(|i| p[1][i] - p[0][i]);
        let v = [0, 1, 2].map(|i| p[2][i] - p[0][i]);
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let normal = if length > 0.0 { n.map(|c| c / length) } else { [0.0; 3] };
        for value in normal.iter().chain(p.iter().flatten()) {
            out.extend_from_slice(&(*value as f32).to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}
