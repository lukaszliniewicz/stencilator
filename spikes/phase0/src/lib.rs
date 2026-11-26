//! Phase 0 spike library.
//!
//! Throwaway code that answered four questions before the engine was written:
//! does the dependency stack build for the browser, does the slab mesher give a
//! closed solid, does an MCP server work, and are traced outlines good enough.
//! All four came out yes. Later work writes the real modules in
//! `stencilator-core` and deletes this crate.
//!
//! Units: 2D and 3D geometry is in integer micrometres, y pointing up.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::items_after_statements,
    clippy::many_single_char_names,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc
)]

pub mod demo;
pub mod digest;
pub mod inspect;
pub mod mesh;
pub mod raster;
pub mod render;
pub mod trace;
