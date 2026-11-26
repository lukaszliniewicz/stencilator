//! The Stencilator engine.
//!
//! It takes artwork and settings and returns plate shapes and files. It never
//! touches the disk, the network, the clock or the screen: the command line,
//! the MCP server and the browser build wrap it and do all input and output.
//! `clippy.toml` in this crate enforces that.
//!
//! Tile mode is complete: a grid of square tiles, a chart to paint from and one
//! stencil per colour. Free-form mode is being built: pictures become maps of
//! paints at a physical size, and plane geometry, the slab mesher and outline
//! tracing are in place.

#![warn(missing_docs)]
#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod artwork;
pub mod chart;
pub mod colour;
mod error;
pub mod export;
pub mod geometry;
pub mod layout;
pub mod mesh;
pub mod paints;
pub mod quantise;
mod resample;
mod rng;
pub mod source;
pub mod stencil;
mod text;
pub mod trace;
pub mod zip;

pub use error::Error;

/// The engine's version, for output files and the command line.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
