//! Spike 2: builds the connector plate, checks it, writes an STL.
//!
//! Usage: `phase0-mesh <rotation-degrees> <out.stl>`

use std::process::ExitCode;

use phase0::{demo, digest, mesh};

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let rotation: f64 = args.get(1).map_or(Ok(0.0), |s| s.parse()).map_err(|e| format!("rotation: {e}"))?;
    let out = args.get(2).ok_or("usage: phase0-mesh <rotation-degrees> <out.stl>")?;
    let slabs = demo::connector_plate(rotation)?;
    let mesh = mesh::mesh_slabs(&slabs)?;
    let report = mesh::check(&mesh);
    let stl = mesh::to_stl(&mesh, "phase0 connector plate");
    std::fs::write(out, &stl).map_err(|e| format!("{out}: {e}"))?;
    println!("{report:#?}");
    println!("single closed solid: {}", report.is_single_closed_solid());
    println!("stl digest: {}", digest::hex(&stl));
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("phase0-mesh: {error}");
            ExitCode::FAILURE
        }
    }
}
