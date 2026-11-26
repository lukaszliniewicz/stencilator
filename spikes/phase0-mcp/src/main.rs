//! Spike 3: the engine behind a local MCP server.
//!
//! An AI client starts this program and talks to it over stdin and stdout;
//! nothing listens on the network. Two tools: one writes a file (the
//! connector plate of spike 2 as STL), one returns an image (a preview of an
//! SVG), so both kinds of result are tried.
//!
//! Try it in Claude Code with `claude mcp add stencilator-spike -- <path to this binary>`.

// The SDK's macros want methods on `self` and generate an async handler.
#![allow(clippy::unused_self, clippy::unused_async_trait_impl)]

use std::io::Cursor;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use phase0::{demo, mesh, render};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

/// The only font the spike knows; Phase 1 takes fonts from the project file.
const FONT: &[u8] = include_bytes!("../../../fonts/NotoSans-Bold.ttf");

#[derive(Debug, Deserialize, JsonSchema)]
struct PlateRequest {
    /// Where to write the STL file; must end in `.stl`.
    path: String,
    /// Rotation of the artwork in degrees, 0 by default.
    #[serde(default)]
    rotation_deg: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PreviewRequest {
    /// Path of an SVG file.
    path: String,
    /// Resolution in pixels per millimetre, 1 to 20; 4 by default.
    px_per_mm: Option<f32>,
}

#[derive(Clone)]
struct Spike;

fn failure(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

#[tool_router]
impl Spike {
    #[tool(description = "Builds the Phase 0 connector plate (a 60 x 40 mm stencil with a ring \
                          held by three bridges), checks that it is one closed solid and writes it as STL.")]
    fn connector_plate(&self, Parameters(request): Parameters<PlateRequest>) -> CallToolResult {
        if !Path::new(&request.path).extension().is_some_and(|e| e.eq_ignore_ascii_case("stl")) {
            return failure("the path must end in .stl");
        }
        if !request.rotation_deg.is_finite() {
            return failure("the rotation must be a number");
        }
        let built = demo::connector_plate(request.rotation_deg).and_then(|slabs| mesh::mesh_slabs(&slabs));
        let plate = match built {
            Ok(plate) => plate,
            Err(error) => return failure(format!("could not build the plate: {error}")),
        };
        let report = mesh::check(&plate);
        if let Err(error) = std::fs::write(&request.path, mesh::to_stl(&plate, "phase0 connector plate")) {
            return failure(format!("could not write {}: {error}", request.path));
        }
        CallToolResult::success(vec![ContentBlock::text(format!(
            "Wrote {}: {} triangles, {:.1} mm3, {}.",
            request.path,
            report.triangles,
            report.volume_mm3,
            if report.is_single_closed_solid() { "one closed solid" } else { "NOT one closed solid" }
        ))])
    }

    #[tool(description = "Renders an SVG file onto white and returns it as a PNG image, with its size in millimetres.")]
    fn preview_svg(&self, Parameters(request): Parameters<PreviewRequest>) -> CallToolResult {
        let px_per_mm = request.px_per_mm.unwrap_or(4.0);
        if !(1.0..=20.0).contains(&px_per_mm) {
            return failure("px_per_mm must be between 1 and 20");
        }
        match preview(&request.path, px_per_mm) {
            Ok((png, summary)) => CallToolResult::success(vec![
                ContentBlock::text(summary),
                ContentBlock::image(BASE64.encode(png), "image/png"),
            ]),
            Err(error) => failure(error),
        }
    }
}

fn preview(path: &str, px_per_mm: f32) -> Result<(Vec<u8>, String), String> {
    let svg = std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;
    let tree = render::parse(&svg, &[FONT])?;
    let (width_mm, height_mm) = render::size_mm(&tree);
    let raster = render::render(&tree, px_per_mm)?;
    let image = image::RgbImage::from_raw(raster.width, raster.height, raster.on_white())
        .ok_or("the render has the wrong size")?;
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    let summary = format!(
        "{path}: {width_mm:.1} x {height_mm:.1} mm, shown at {px_per_mm} px/mm ({} x {} pixels).",
        raster.width, raster.height
    );
    Ok((png.into_inner(), summary))
}

#[tool_handler]
impl ServerHandler for Spike {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("stencilator-phase0", env!("CARGO_PKG_VERSION")))
            .with_instructions("Phase 0 spike of the stencil engine. Paths are on this computer.")
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = Spike.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
