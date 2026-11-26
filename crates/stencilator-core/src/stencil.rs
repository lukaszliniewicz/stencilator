//! Tile-mode stencils: one plate per colour with a square hole over each tile.
//!
//! The plate covers the whole canvas, so its edges register against the board. The grid
//! lines become the web that holds the plate together, which is why a stencil needs a grid
//! line width above zero.
//!
//! The mesh is built on the lattice of grid-line and tile edges. Every horizontal band of
//! the plate is triangulated so that its vertices match its neighbours' exactly, which
//! makes the mesh closed and consistently wound (every edge shared by exactly two
//! triangles, no T-junctions), with a triangle count that grows with the number of holes,
//! not the grid area. Free-form plates will use slabs instead.

use std::ops::RangeInclusive;

use crate::Error;
use crate::chart::TileChart;
use crate::colour::Rgb;
use crate::layout::{GridLayout, MM_PER_INCH};

/// Thinnest web worth printing: about 1.5 extrusion widths of a 0.4 mm nozzle.
pub const RECOMMENDED_MIN_WEB_MM: f64 = 0.6;

/// Accepted plate thicknesses in millimetres.
pub const THICKNESS_MM: RangeInclusive<f64> = 0.2..=20.0;

/// A triangle's three corners in millimetres, counter-clockwise seen from outside.
pub type Triangle = [[f64; 3]; 3];

/// Lattice point: x index, y index (and z index for walls).
type Point2 = (u32, u32);
type Point3 = (u32, u32, u32);

/// The file name of a colour's stencil, such as `stencil_color_007_065_200.stl`.
pub fn stencil_filename(colour: Rgb) -> String {
    let [r, g, b] = colour;
    format!("stencil_color_{r:03}_{g:03}_{b:03}.stl")
}

/// The stencil of palette colour `index` as binary STL, with its [`stencil_filename`].
///
/// # Errors
///
/// [`Error::Stencil`] when there is no such colour, or as [`stencil_triangles`].
pub fn stencil_file(chart: &TileChart, index: u32, thickness_mm: f64) -> Result<(String, Vec<u8>), Error> {
    let &[r, g, b] = chart
        .palette
        .get(index as usize)
        .ok_or_else(|| Error::Stencil(format!("There is no colour {} in the palette.", u64::from(index) + 1)))?;
    let holes: Vec<bool> = chart.labels.iter().map(|&l| l == index).collect();
    let triangles = stencil_triangles(&holes, &chart.layout, thickness_mm)?;
    Ok((stencil_filename([r, g, b]), encode_stl(&triangles, &format!("stencil rgb({r}, {g}, {b})"))))
}

/// Every colour's stencil, in palette order, as by [`stencil_file`].
///
/// # Errors
///
/// As [`stencil_triangles`].
pub fn stencil_files(chart: &TileChart, thickness_mm: f64) -> Result<Vec<(String, Vec<u8>)>, Error> {
    (0..).zip(&chart.palette).map(|(index, _)| stencil_file(chart, index, thickness_mm)).collect()
}

/// A closed plate mesh with a through-hole for every `true` tile.
///
/// `holes` has one entry per tile, row by row from the top of the image. The mesh lies
/// on z = 0 to `thickness_mm`, with the image the right way up seen from above.
///
/// # Errors
///
/// [`Error::Stencil`] when the grid has no lines, the thickness is outside
/// [`THICKNESS_MM`], or the hole mask does not match the grid.
pub fn stencil_triangles(holes: &[bool], layout: &GridLayout, thickness_mm: f64) -> Result<Vec<Triangle>, Error> {
    if layout.line_px == 0 {
        return Err(Error::Stencil(
            "Stencils need grid lines: without them the holes merge and parts fall out.".to_owned(),
        ));
    }
    check_thickness(thickness_mm)?;
    let (tiles_x, tiles_y) = (layout.tiles_x as usize, layout.tiles_y as usize);
    if holes.len() != tiles_x * tiles_y {
        return Err(Error::Stencil("The hole mask does not match the grid.".to_owned()));
    }

    // STL's y axis points up, image rows go down: lattice row 0 is the image's last row.
    let hole_cols: Vec<Vec<u32>> = (0..tiles_y)
        .rev()
        .map(|row| (0..).zip(&holes[row * tiles_x..(row + 1) * tiles_x]).filter(|(_, h)| **h).map(|(i, _)| i).collect())
        .collect();
    let (faces, walls) = lattice_mesh(&hole_cols, layout.tiles_x);

    let xs = lattice_positions_mm(layout.tiles_x, layout);
    let ys = lattice_positions_mm(layout.tiles_y, layout);
    let zs = [0.0, thickness_mm];
    let point = |(x, y, z): Point3| [xs[x as usize], ys[y as usize], zs[z as usize]];
    let top = faces.iter().map(|&[a, b, c]| [(a.0, a.1, 1), (b.0, b.1, 1), (c.0, c.1, 1)]);
    let bottom = faces.iter().map(|&[a, b, c]| [(a.0, a.1, 0), (c.0, c.1, 0), (b.0, b.1, 0)]);
    Ok(top.chain(bottom).chain(walls).map(|t| t.map(point)).collect())
}

/// Checks a plate thickness against [`THICKNESS_MM`].
///
/// # Errors
///
/// [`Error::Stencil`] when it is out of range.
pub fn check_thickness(thickness_mm: f64) -> Result<(), Error> {
    if THICKNESS_MM.contains(&thickness_mm) {
        Ok(())
    } else {
        Err(Error::Stencil(format!(
            "The stencil thickness must be between {} and {} mm.",
            THICKNESS_MM.start(),
            THICKNESS_MM.end()
        )))
    }
}

/// Lattice line positions: an even index starts a grid line, an odd index a tile.
#[allow(clippy::cast_precision_loss, reason = "pixel counts are far below 2^52")]
fn lattice_positions_mm(n_tiles: u32, layout: &GridLayout) -> Vec<f64> {
    (0..2 * u64::from(n_tiles) + 2)
        .map(|k| {
            let px = (k / 2) * layout.pitch_px() + (k % 2) * u64::from(layout.line_px);
            px as f64 / f64::from(layout.dpi) * MM_PER_INCH
        })
        .collect()
}

/// Top-face triangles (counter-clockwise seen from above) and wall triangles on the
/// lattice. `hole_cols[m]` lists the hole columns of lattice tile row `m`, from the bottom.
/// Lattice index 2i+1 to 2i+2 spans tile i; even indices start grid lines.
fn lattice_mesh(hole_cols: &[Vec<u32>], tiles_x: u32) -> (Vec<[Point2; 3]>, Vec<[Point3; 3]>) {
    let tiles_y = u32::try_from(hole_cols.len()).unwrap_or(u32::MAX);
    let (last_x, last_y) = (2 * tiles_x + 1, 2 * tiles_y + 1);
    // x indices on the edge between a grid-line band and tile row `row`, if any.
    let edge_vertices = |row: Option<u32>| -> Vec<u32> {
        let cols = row.and_then(|r| hole_cols.get(r as usize)).map_or(&[][..], Vec::as_slice);
        std::iter::once(0).chain(cols.iter().flat_map(|&i| [2 * i + 1, 2 * i + 2])).chain([last_x]).collect()
    };

    let mut faces = Vec::new();
    let mut walls = Vec::new();
    // Grid-line bands are solid across the full width; zip their two vertex chains together.
    for m in 0..=tiles_y {
        faces.extend(zip_strip(&edge_vertices(m.checked_sub(1)), &edge_vertices(Some(m)), 2 * m, 2 * m + 1));
    }
    // Tile rows: solid runs between holes, and four walls around every hole.
    for (m, cols) in (0..).zip(hole_cols) {
        let (y0, y1) = (2 * m + 1, 2 * m + 2);
        let mut run_start = 0;
        for &i in cols {
            let (x0, x1) = (2 * i + 1, 2 * i + 2);
            faces.extend(quad(run_start, x0, y0, y1));
            run_start = x1;
            // Walk the hole clockwise so the solid is on the left of every wall.
            for (p, q) in [((x0, y0), (x0, y1)), ((x0, y1), (x1, y1)), ((x1, y1), (x1, y0)), ((x1, y0), (x0, y0))] {
                walls.extend(wall(p, q));
            }
        }
        faces.extend(quad(run_start, last_x, y0, y1));
    }
    // Outer boundary, counter-clockwise. The sides have a vertex at every band edge.
    walls.extend(wall((0, 0), (last_x, 0)));
    walls.extend(wall((last_x, last_y), (0, last_y)));
    for k in 0..last_y {
        walls.extend(wall((last_x, k), (last_x, k + 1)));
        walls.extend(wall((0, k + 1), (0, k)));
    }
    (faces, walls)
}

fn quad(x0: u32, x1: u32, y0: u32, y1: u32) -> [[Point2; 3]; 2] {
    [[(x0, y0), (x1, y0), (x1, y1)], [(x0, y0), (x1, y1), (x0, y1)]]
}

/// Triangulates a rectangle whose bottom and top edges carry extra collinear vertices.
fn zip_strip(bottom: &[u32], top: &[u32], y0: u32, y1: u32) -> Vec<[Point2; 3]> {
    let mut triangles = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < bottom.len() - 1 || j < top.len() - 1 {
        if j == top.len() - 1 || (i < bottom.len() - 1 && bottom[i + 1] <= top[j + 1]) {
            triangles.push([(bottom[i], y0), (bottom[i + 1], y0), (top[j], y1)]);
            i += 1;
        } else {
            triangles.push([(bottom[i], y0), (top[j + 1], y1), (top[j], y1)]);
            j += 1;
        }
    }
    triangles
}

/// A vertical wall along `p` to `q`, facing right of that direction (solid on the left).
fn wall(p: Point2, q: Point2) -> [[Point3; 3]; 2] {
    let (p0, q0, p1, q1) = ((p.0, p.1, 0), (q.0, q.1, 0), (p.0, p.1, 1), (q.0, q.1, 1));
    [[p0, q0, q1], [p0, q1, p1]]
}

/// Binary STL with unit normals computed from the winding.
#[allow(clippy::cast_possible_truncation, reason = "STL stores single precision")]
pub fn encode_stl(triangles: &[Triangle], header: &str) -> Vec<u8> {
    // A binary header must not start with "solid", or some readers take the file for ASCII.
    let mut head: Vec<u8> =
        format!("stencilator {header}").bytes().map(|b| if b.is_ascii() { b } else { b'?' }).take(80).collect();
    head.resize(80, b' ');
    let count = u32::try_from(triangles.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(84 + triangles.len() * 50);
    out.extend_from_slice(&head);
    out.extend_from_slice(&count.to_le_bytes());
    for t in triangles {
        let (u, v) = (sub(t[1], t[0]), sub(t[2], t[0]));
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let normal = if length > 0.0 { n.map(|c| c / length) } else { [0.0; 3] };
        for value in normal.iter().chain(t.iter().flatten()) {
            out.extend_from_slice(&(*value as f32).to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn layout(tiles_x: u32, tiles_y: u32) -> GridLayout {
        GridLayout { tiles_x, tiles_y, tile_px: 10, line_px: 2, dpi: 254 }
    }

    /// Every directed edge must meet its reverse exactly once: closed and consistently wound.
    fn assert_closed(triangles: &[Triangle]) {
        let key = |p: [f64; 3]| p.map(f64::to_bits);
        let mut edges: BTreeMap<([u64; 3], [u64; 3]), i32> = BTreeMap::new();
        for t in triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *edges.entry((key(a), key(b))).or_default() += 1;
            }
        }
        for (&(a, b), &n) in &edges {
            assert_eq!(n, 1, "edge used {n} times");
            assert_eq!(edges.get(&(b, a)), Some(&1), "open edge");
        }
    }

    fn volume(triangles: &[Triangle]) -> f64 {
        triangles
            .iter()
            .map(|[a, b, c]| {
                a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                    + a[2] * (b[0] * c[1] - b[1] * c[0])
            })
            .sum::<f64>()
            / 6.0
    }

    #[test]
    #[allow(clippy::cast_precision_loss, reason = "small pixel counts")]
    fn every_hole_pattern_gives_a_closed_plate_of_the_right_volume() {
        let g = layout(4, 3);
        for pattern in 0_u32..(1 << 12) {
            if pattern % 7 != 0 {
                continue; // a spread of patterns is enough
            }
            let holes: Vec<bool> = (0..12).map(|i| pattern >> i & 1 == 1).collect();
            let triangles = stencil_triangles(&holes, &g, 1.5).unwrap();
            assert_closed(&triangles);
            let mm = 0.1; // 254 DPI: one pixel is 0.1 mm
            let n_holes = f64::from(pattern.count_ones());
            let expected = (g.width_px() as f64 * mm) * (g.height_px() as f64 * mm) * 1.5 - n_holes * 1.0 * 1.0 * 1.5;
            assert!((volume(&triangles) - expected).abs() < 1e-9, "{pattern:b}");
        }
    }

    /// Whether the top face covers a point.
    fn covered(triangles: &[Triangle], x: f64, y: f64) -> bool {
        triangles.iter().filter(|t| t.iter().all(|p| p[2] > 0.0)).any(|t| {
            let side = |a: [f64; 3], b: [f64; 3]| (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
            side(t[0], t[1]) >= 0.0 && side(t[1], t[2]) >= 0.0 && side(t[2], t[0]) >= 0.0
        })
    }

    #[test]
    fn the_image_is_the_right_way_up() {
        // A hole in the image's top-left tile: seen from above, at small x and large y.
        let triangles = stencil_triangles(&[true, false, false, false], &layout(2, 2), 1.0).unwrap();
        assert!(!covered(&triangles, 0.7, 1.9));
        assert!(covered(&triangles, 0.7, 0.7));
        assert!(covered(&triangles, 1.9, 1.9));
        assert!(covered(&triangles, 0.1, 1.9)); // the web
    }

    #[test]
    fn bad_requests_are_refused() {
        let no_lines = GridLayout { line_px: 0, ..layout(2, 2) };
        assert!(stencil_triangles(&[false; 4], &no_lines, 1.0).is_err());
        assert!(stencil_triangles(&[false; 4], &layout(2, 2), 0.0).is_err());
        assert!(stencil_triangles(&[false; 4], &layout(2, 2), 1e300).is_err());
        assert!(stencil_triangles(&[false; 3], &layout(2, 2), 1.0).is_err());
    }

    #[test]
    fn stl_has_a_record_per_triangle() {
        let triangles = stencil_triangles(&[true, false], &layout(2, 1), 1.0).unwrap();
        let stl = encode_stl(&triangles, "test");
        assert_eq!(stl.len(), 84 + 50 * triangles.len());
        assert!(stl.starts_with(b"stencilator test"));
        assert_eq!(u32::from_le_bytes(stl[80..84].try_into().unwrap()) as usize, triangles.len());
    }

    #[test]
    fn file_names_carry_the_colour() {
        assert_eq!(stencil_filename([7, 65, 200]), "stencil_color_007_065_200.stl");
    }
}
