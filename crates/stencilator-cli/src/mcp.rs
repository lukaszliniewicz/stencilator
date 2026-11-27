//! Local MCP access to the same project builds, reports and exports as the CLI.

mod design;
mod workspace;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use clap::Args;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::service::{RequestContext, RoleServer, ServerInitializeError};
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
use tokio::sync::Mutex;

use self::design::{Design, extension};
use self::workspace::{MAX_INPUT_BYTES, Workspace};
use crate::{freeform, tiles};

const CANCELLED: &str = "The stencil request was cancelled.";

#[derive(Args)]
pub(crate) struct McpArgs {
    /// Folder containing the artwork, projects and exports. Tool paths are relative to it.
    #[arg(long, value_name = "FOLDER")]
    workspace: PathBuf,
}

pub(crate) fn run(args: &McpArgs) -> Result<(), String> {
    let server = Server { workspace: Arc::new(Mutex::new(Workspace::new(&args.workspace)?)) };
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|error| error.to_string())?;
    let result = runtime.block_on(async {
        match server.serve(rmcp::transport::stdio_pipes()).await {
            Ok(service) => service.waiting().await.map_err(|error| error.to_string())?,
            Err(ServerInitializeError::ConnectionClosed(_)) => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        Ok(())
    });
    // The SDK has drained responses and cancelled any remaining requests.
    // Don't keep a disconnected client waiting for an entire engine build.
    runtime.shutdown_timeout(Duration::from_secs(1));
    result
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
    workspace: Arc<Mutex<Workspace>>,
}

#[tool_router]
impl Server {
    #[tool(
        description = "Check a tile or free-form stencil design. Report the paints, dimensions, print-bed fit and printability warnings without writing files.",
        annotations(title = "Check stencils", read_only_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn check_stencil(
        &self,
        Parameters(request): Parameters<CheckRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run_tool(context, move |workspace, context| {
            let (project, built) = build(workspace, request.design, context)?;
            active(context)?;
            Ok(built.report(&project, &[]))
        })
        .await
    }

    #[tool(
        description = "Export one new file for a stencil design and report what may not print well. A .stencilator file opens in the web app. A .zip or free-form .3mf includes any registration frame. Existing files are never overwritten.",
        annotations(
            title = "Export a stencil file",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn export_stencil(
        &self,
        Parameters(request): Parameters<ExportRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run_tool(context, move |workspace, context| export(workspace, request, context)).await
    }
}

#[tool_handler]
#[allow(clippy::unused_async_trait_impl, reason = "the SDK generates an async tools/list handler")]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("stencilator", VERSION))
            .with_instructions(
                "Make painting stencils locally. check_stencil reports printability without writing files. \
                 export_stencil creates one new file; use .stencilator for a project the web app can open. \
                 All tool paths are relative to the workspace chosen at launch. Tile mode takes pictures; \
                 free-form also takes SVG and text.",
            )
    }
}

impl Server {
    async fn run_tool(
        &self,
        context: RequestContext<RoleServer>,
        work: impl FnOnce(&Workspace, &RequestContext<RoleServer>) -> Result<Value, String> + Send + 'static,
    ) -> CallToolResult {
        // One build at a time to bound memory use. The blocking worker keeps
        // its slot until it exits, while stdio can still receive cancellation.
        let workspace = tokio::select! {
            workspace = Arc::clone(&self.workspace).lock_owned() => workspace,
            () = context.ct.cancelled() => return tool_result(Err(CANCELLED.to_owned())),
        };
        let result = tokio::task::spawn_blocking(move || {
            active(&context)?;
            work(&workspace, &context).map(compact)
        })
        .await
        .unwrap_or_else(|_| Err("The stencil build stopped unexpectedly. Try again with a simpler design.".to_owned()));
        tool_result(result)
    }
}

fn active(context: &RequestContext<RoleServer>) -> Result<(), String> {
    if context.ct.is_cancelled() {
        return Err(CANCELLED.to_owned());
    }
    Ok(())
}

fn build(
    workspace: &Workspace,
    design: Design,
    context: &RequestContext<RoleServer>,
) -> Result<(Project, Built), String> {
    let project = design.project(workspace)?;
    active(context)?;
    let built = match project.mode {
        Mode::Tiles => Built::Tiles(tiles::build(&project)?),
        Mode::Freeform => Built::Freeform(freeform::build(&project)?),
    };
    Ok((project, built))
}

fn export(
    workspace: &Workspace,
    request: ExportRequest,
    context: &RequestContext<RoleServer>,
) -> Result<Value, String> {
    let path = &request.path;
    let format = ExportFormat::from_path(path)?;
    let destination = workspace.destination(path)?;
    let (project, built) = build(workspace, request.design, context)?;
    active(context)?;
    let bytes = built.file(&project, format)?;
    active(context)?;
    let report = built.report(&project, &[path.as_path()]);
    active(context)?;
    workspace.write(&destination, &bytes, || active(context))?;
    Ok(report)
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
        Ok(report) => CallToolResult::structured(report),
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
