//! Browser build of the Stencilator engine, called from the app's Web Worker.

use wasm_bindgen::prelude::wasm_bindgen;

/// The engine version.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    stencilator_core::VERSION.to_owned()
}
