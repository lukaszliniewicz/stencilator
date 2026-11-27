//! Artwork and project settings at the MCP boundary.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};
use stencilator_core::project::{
    BUILT_IN_FONT, Colour, Font, Project, STENCIL_FONT, Settings, Source, TEXT_MM, read_project,
};

use super::workspace::Workspace;
use crate::{parse_colour, sources};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct Design {
    source: Artwork,
    /// Change only the given settings, with the same names and units as a project file.
    /// An array replaces the old array. Use null to clear an optional setting, such as
    /// `registration_clearance_mm`; use `"size": "natural"` for an SVG or text at its own size.
    #[schemars(with = "Option<Settings>")]
    settings: Option<Map<String, Value>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Artwork {
    /// Open a PNG, JPEG, WebP, SVG or .stencilator file.
    File {
        /// Path relative to the workspace.
        path: PathBuf,
    },
    /// Open a Stencilator project saved under any file name.
    Project {
        /// Path relative to the workspace.
        path: PathBuf,
    },
    /// Make a free-form design from the given SVG markup.
    Svg {
        /// A self-contained SVG, up to 1 MiB. External links are ignored.
        svg: String,
    },
    /// Make a free-form design from text, centred line by line.
    Text {
        /// Newlines separate lines of text.
        text: String,
        /// Font size in millimetres, 30 by default.
        font_mm: Option<f64>,
        /// Hex colour such as "#1d2f5c"; black by default.
        colour: Option<String>,
        /// Built-in font. Defaults to Noto Sans Bold.
        font: Option<BuiltInFont>,
    },
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum BuiltInFont {
    NotoSansBold,
    AllertaStencil,
}

impl Design {
    pub(super) fn project(self, workspace: &Workspace) -> Result<Project, String> {
        let mut project = self.source.load(workspace)?;
        if let Some(settings) = self.settings {
            apply_settings(&mut project, settings)?;
        }
        project.check().map_err(|error| error.to_string())?;
        Ok(project)
    }
}

impl Artwork {
    fn load(self, workspace: &Workspace) -> Result<Project, String> {
        let open_project =
            |path: &Path| read_project(&workspace.read(path)?).map_err(|error| format!("{}: {error}", path.display()));
        match self {
            Self::File { path } => match extension(&path).as_deref() {
                Some("stencilator") => open_project(&path),
                Some("png" | "jpg" | "jpeg" | "webp" | "svg") => Ok(sources::artwork(&path, workspace.read(&path)?)),
                _ => Err("Use a PNG, JPEG, WebP, SVG or .stencilator file for the artwork.".to_owned()),
            },
            Self::Project { path } => open_project(&path),
            Self::Svg { svg } => {
                if svg.len() > 1024 * 1024 {
                    return Err(
                        "Inline SVG is limited to 1 MiB. Save the SVG in the workspace and open the file.".to_owned()
                    );
                }
                Ok(sources::artwork(Path::new("drawing.svg"), svg.into_bytes()))
            }
            Self::Text { text, font_mm, colour, font } => {
                if text.len() > 16 * 1024 {
                    return Err("Typed text is limited to 16 KiB.".to_owned());
                }
                let colour = Colour(colour.as_deref().map(parse_colour).transpose()?.unwrap_or([0; 3]));
                let mut project = Project::new(Source::Text { text, font_mm: font_mm.unwrap_or(TEXT_MM), colour });
                if let Some(font) = font {
                    let name = match font {
                        BuiltInFont::NotoSansBold => BUILT_IN_FONT,
                        BuiltInFont::AllertaStencil => STENCIL_FONT,
                    };
                    project.fonts.push(Font { name: name.to_owned(), data: None });
                }
                Ok(project)
            }
        }
    }
}

fn apply_settings(project: &mut Project, patch: Map<String, Value>) -> Result<(), String> {
    let mut effective = serde_json::to_value(project.settings()).map_err(|error| error.to_string())?;
    overlay(&mut effective, Value::Object(patch));
    let mut ignored = None;
    let settings: Settings = serde_ignored::deserialize(effective, |field| {
        ignored.get_or_insert_with(|| field.to_string());
    })
    .map_err(|error| format!("Check the settings: {error}."))?;
    if let Some(field) = ignored {
        return Err(format!("Unknown setting: {field}."));
    }
    project.mode = settings.mode;
    project.colours = settings.colours;
    project.tiles = settings.tiles;
    project.freeform = settings.freeform;
    project.stencils = settings.stencils;
    Ok(())
}

fn overlay(current: &mut Value, patch: Value) {
    match (current, patch) {
        (Value::Object(existing), Value::Object(updates)) => {
            for (key, value) in updates {
                // Size is one choice (for example width or height), not a settings group.
                if let Some(previous) = existing.get_mut(&key).filter(|_| key != "size") {
                    overlay(previous, value);
                } else {
                    existing.insert(key, value);
                }
            }
        }
        (current, replacement) => *current = replacement,
    }
}

pub(super) fn extension(path: &Path) -> Option<String> {
    path.extension().and_then(|suffix| suffix.to_str()).map(str::to_ascii_lowercase)
}
