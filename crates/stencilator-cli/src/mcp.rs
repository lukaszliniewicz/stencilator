//! Local MCP access to the same project builds, reports and exports as the CLI.

mod design;
mod workspace;

use std::path::{Path, PathBuf};

use clap::Args;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use stencilator_core::VERSION;
use stencilator_core::chart::TileChart;
use stencilator_core::export::{ChartFormat, encode_chart};
use stencilator_core::plate::{plate_files, plates_3mf};
use stencilator_core::project::{Mode, Project};
use stencilator_core::stencil::stencil_files;
use stencilator_core::zip::zip;

use self::design::{Design, extension};
use self::workspace::{MAX_INPUT_BYTES, Workspace};
use crate::{freeform, tiles};

#[derive(Args)]
pub(crate) struct McpArgs {
    /// Folder containing the artwork, projects and exports. Tool paths are relative to it.
    #[arg(long, value_name = "FOLDER")]
    workspace: PathBuf,
}

pub(crate) fn run(args: &McpArgs) -> Result<(), String> {
    let server = Server { workspace: Workspace::new(&args.workspace)? };
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let service = server.serve(rmcp::transport::stdio()).await.map_err(|error| error.to_string())?;
        service.waiting().await.map_err(|error| error.to_string())?;
        Ok(())
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CheckRequest {
    design: Design,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExportRequest {
    design: Design,
    /// New file, relative to the workspace, in an existing folder. Use .stencilator for
    /// an editable project, .json for the full report, .zip for STLs, .3mf for free-form,
    /// or .pdf, .png or .jpg for a tile chart.
    path: PathBuf,
}

#[derive(Clone)]
struct Server {
    workspace: Workspace,
}

#[tool_router]
impl Server {
    #[tool(
        description = "Check a tile or free-form stencil design. Report the paints, dimensions, print-bed fit and printability warnings without writing files.",
        annotations(title = "Check stencils", read_only_hint = true, open_world_hint = false)
    )]
    fn check_stencil(&self, Parameters(request): Parameters<CheckRequest>) -> CallToolResult {
        tool_result(self.build(request.design).map(|(project, built)| built.report(&project, &[])))
    }

    #[tool(
        description = "Export one new file for a stencil design and report what may not print well. A .stencilator file opens in the web app. A .zip or free-form .3mf includes any registration frame. Existing files are never overwritten.",
        annotations(
            title = "Export a stencil file",
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn export_stencil(&self, Parameters(request): Parameters<ExportRequest>) -> CallToolResult {
        tool_result(self.export(request))
    }
}

#[tool_handler]
#[allow(clippy::unused_async_trait_impl, reason = "the MCP SDK generates the handler for the synchronous tools")]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("stencilator", VERSION))
            .with_instructions(
                "Make painting stencils locally. Use check_stencil to inspect a design before printing. \
                 Tile mode takes a picture; free-form takes pictures, SVG and text. Tool paths are relative \
                 to the workspace chosen at launch. Export a .stencilator project to open it in the web app.",
            )
    }
}

impl Server {
    fn build(&self, design: Design) -> Result<(Project, Built), String> {
        let project = design.project(&self.workspace)?;
        let built = match project.mode {
            Mode::Tiles => Built::Tiles(tiles::build(&project)?),
            Mode::Freeform => Built::Freeform(freeform::build(&project)?),
        };
        Ok((project, built))
    }

    fn export(&self, request: ExportRequest) -> Result<Value, String> {
        let path = &request.path;
        let format = ExportFormat::from_path(path)?;
        let destination = self.workspace.destination(path)?;
        let (project, built) = self.build(request.design)?;
        let bytes = built.file(&project, format)?;
        let report = built.report(&project, &[path.as_path()]);
        self.workspace.write(path, &destination, &bytes)?;
        Ok(report)
    }
}

enum ExportFormat {
    Project,
    Report,
    StlZip,
    ThreeMf,
    Chart(ChartFormat),
}

impl ExportFormat {
    fn from_path(path: &Path) -> Result<Self, String> {
        match extension(path).as_deref() {
            Some("stencilator") => Ok(Self::Project),
            Some("json") => Ok(Self::Report),
            Some("zip") => Ok(Self::StlZip),
            Some("3mf") => Ok(Self::ThreeMf),
            Some("pdf" | "png" | "jpg" | "jpeg") => {
                ChartFormat::from_file_name(&path.to_string_lossy()).map(Self::Chart).map_err(|error| error.to_string())
            }
            _ => Err("Use .stencilator, .json, .zip, .3mf or a tile chart (.pdf, .png or .jpg).".to_owned()),
        }
    }
}

enum Built {
    Tiles(TileChart),
    Freeform(freeform::Built),
}

impl Built {
    fn report(&self, project: &Project, written: &[&Path]) -> Value {
        match self {
            Self::Tiles(chart) => tiles::Summary::new(chart, project, &project.stencil_skip(), written).json(),
            Self::Freeform(built) => {
                let report = built.report(project, written);
                report.json(&report.warnings())
            }
        }
    }

    fn file(&self, project: &Project, format: ExportFormat) -> Result<Vec<u8>, String> {
        let thickness = project.stencils.thickness_mm;
        let result = match (self, format) {
            (_, ExportFormat::Project) => {
                let bytes = project.to_json();
                if bytes.len() as u64 > MAX_INPUT_BYTES {
                    return Err("The saved project would exceed 128 MiB. Use a smaller source file.".to_owned());
                }
                return Ok(bytes);
            }
            (_, ExportFormat::Report) => {
                let mut bytes =
                    serde_json::to_vec_pretty(&self.report(project, &[])).map_err(|error| error.to_string())?;
                bytes.push(b'\n');
                return Ok(bytes);
            }
            (Self::Tiles(chart), ExportFormat::Chart(format)) => {
                encode_chart(chart, format, project.tiles.grid_colour.0, true)
            }
            (Self::Tiles(chart), ExportFormat::StlZip) => {
                stencil_files(chart, thickness, &project.stencil_skip()).and_then(|files| zip(&files))
            }
            (Self::Freeform(built), ExportFormat::ThreeMf | ExportFormat::StlZip) if built.plates.is_empty() => {
                return Err("Every paint is skipped, so there are no plates to make.".to_owned());
            }
            (Self::Freeform(built), ExportFormat::ThreeMf) => plates_3mf(&built.plates, thickness),
            (Self::Freeform(built), ExportFormat::StlZip) => {
                plate_files(&built.plates, thickness).and_then(|files| zip(&files))
            }
            (Self::Tiles(_), ExportFormat::ThreeMf) => {
                return Err("3MF is for free-form designs. Use .zip in tile mode.".to_owned());
            }
            (Self::Freeform(_), ExportFormat::Chart(_)) => return Err("Painting charts are for tile mode.".to_owned()),
        };
        result.map_err(|error| error.to_string())
    }
}

fn tool_result(result: Result<Value, String>) -> CallToolResult {
    match result {
        Ok(report) => CallToolResult::structured(compact(report)),
        Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
    }
}

fn compact(mut report: Value) -> Value {
    // Complex art can put thousands of feature locations in the report. The full
    // report is available as a file; keep the tool result small enough to read.
    if report.to_string().len() > 32 * 1024 {
        for (field, count, keep) in
            [("palette", "paint_count", 8), ("plates", "plate_count", 0), ("warnings", "warning_count", 12)]
        {
            if let Some(items) = report.get_mut(field).and_then(Value::as_array_mut) {
                let total = items.len();
                items.truncate(keep);
                report[count] = Value::from(total);
            }
        }
        report["report_truncated"] = Value::Bool(true);
    }
    report
}
