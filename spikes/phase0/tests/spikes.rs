//! The Phase 0 results, kept as tests so the spikes stay honest while they
//! live in the tree.

use phase0::mesh::{self, Pt, Shapes, Slab};
use phase0::trace::LabelMap;
use phase0::{demo, digest, render, trace};

const FONT: &[u8] = include_bytes!("../../../fixtures/fonts/NotoSans-Bold.ttf");
const BADGE: &str = include_str!("../../../fixtures/svg/badge.svg");

fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<Pt> {
    vec![Pt::new(x0, y0), Pt::new(x1, y0), Pt::new(x1, y1), Pt::new(x0, y1)]
}

/// Area in square micrometres; holes count negative.
fn area_um2(shapes: &Shapes) -> f64 {
    shapes
        .iter()
        .flatten()
        .map(|ring| {
            let n = ring.len();
            (0..n)
                .map(|i| {
                    let (a, b) = (ring[i], ring[(i + 1) % n]);
                    f64::from(a.x) * f64::from(b.y) - f64::from(b.x) * f64::from(a.y)
                })
                .sum::<f64>()
                / 2.0
        })
        .sum()
}

#[test]
fn fnv1a_matches_published_values() {
    assert_eq!(digest::fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(digest::fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
}

#[test]
fn connector_plate_is_one_closed_solid_at_any_rotation() {
    for rotation in [0.0, 7.0, 13.3, 33.3, 45.0, 90.0, 121.7, 200.0] {
        let plate = mesh::mesh_slabs(&demo::connector_plate(rotation).unwrap()).unwrap();
        let report = mesh::check(&plate);
        assert!(report.is_single_closed_solid(), "rotation {rotation}: {report:?}");
        // A ring opening joined by three bridges: genus 4.
        assert_eq!(report.euler, -6, "rotation {rotation}");
        assert!((report.volume_mm3 - 2494.0).abs() < 5.0, "rotation {rotation}: {} mm3", report.volume_mm3);
    }
}

#[test]
fn plate_with_a_hole_is_a_closed_ring() {
    let ring = vec![vec![rect(0, 0, 10_000, 10_000), rect(3000, 3000, 7000, 7000).into_iter().rev().collect()]];
    let plate = mesh::mesh_slabs(&[Slab { z0: 0, z1: 1000, shapes: ring }]).unwrap();
    let report = mesh::check(&plate);
    assert!(report.is_single_closed_solid(), "{report:?}");
    assert_eq!(report.euler, 0);
    assert!((report.volume_mm3 - 84.0).abs() < 1e-9);
}

#[test]
fn stacked_slabs_with_different_outlines_stay_closed() {
    let wide = vec![vec![rect(0, 0, 20_000, 20_000)]];
    let narrow = vec![vec![rect(5000, 5000, 15_000, 15_000)]];
    let offset = vec![vec![rect(12_000, 12_000, 19_000, 19_000)]];
    let slabs = [
        Slab { z0: 0, z1: 600, shapes: wide },
        Slab { z0: 600, z1: 1800, shapes: narrow.clone() },
        Slab { z0: 1800, z1: 2400, shapes: mesh::boolean(&narrow, &offset, mesh::Rule::Union) },
    ];
    let report = mesh::check(&mesh::mesh_slabs(&slabs).unwrap());
    assert!(report.is_single_closed_solid(), "{report:?}");
    assert_eq!(report.euler, 2);
}

#[test]
fn traced_block_has_the_block_area() {
    // A 30 x 20 pixel block at 10 px/mm: 3 x 2 mm.
    let (width, height) = (50, 40);
    let mut labels = vec![0_u8; width * height];
    for y in 10..30 {
        for x in 10..40 {
            labels[y * width + x] = 1;
        }
    }
    let map = LabelMap { width, height, labels };
    let shapes = trace::trace(&map, 1, 100.0, 4000.0, 1, 20.0);
    assert_eq!(shapes.len(), 1);
    assert_eq!(shapes[0].len(), 1);
    let area_mm2 = area_um2(&shapes) / 1e6;
    // The blur rounds the four corners slightly.
    assert!((area_mm2 - 6.0).abs() < 0.05, "{area_mm2} mm2");
}

#[test]
fn badge_renders_and_traces_the_same_every_time() {
    let trace_digest = || {
        let tree = render::parse(BADGE, &[FONT]).unwrap();
        let raster = render::render(&tree, 10.0).unwrap();
        let palette = trace::palette(&raster, 0.005);
        let map = trace::classify(&raster, &palette);
        let shapes = trace::trace(&map, 1, 100.0, 70_000.0, 1, 20.0);
        let bytes: Vec<u8> =
            shapes.iter().flatten().flatten().flat_map(|p| [p.x.to_le_bytes(), p.y.to_le_bytes()].concat()).collect();
        (palette, digest::hex(&raster.rgba), digest::hex(&bytes))
    };
    let (palette, render_digest, outline_digest) = trace_digest();
    assert_eq!(palette, [[0xef, 0x7d, 0x2d], [0x1d, 0x2f, 0x5c]]);
    // The digests the browser build reproduced in Phase 0.
    assert_eq!(render_digest, "874b083f11792ba5");
    assert_eq!(outline_digest, "d252a593ab8fbfde");
    assert_eq!(trace_digest(), (palette, render_digest, outline_digest));
}
