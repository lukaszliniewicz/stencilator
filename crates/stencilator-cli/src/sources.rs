//! Where a project comes from: a project file, a picture, an SVG drawing or typed text,
//! and the fonts its text needs.

use std::path::{Path, PathBuf};

use stencilator_core::project::{EXTENSION, Font, Project, Source, read_project};

/// The built-in font, [`stencilator_core::project::BUILT_IN_FONT`].
const BUILT_IN: &[u8] = include_bytes!("../../../fonts/NotoSans-Bold.ttf");

/// What the command was given to work from.
pub(crate) enum Input {
    /// A project saved earlier.
    Project(PathBuf),
    /// A picture or a drawing, read once the settings are known to be in range.
    File(PathBuf),
    /// Typed text, one line per item.
    Text(Vec<String>),
}

impl Input {
    pub(crate) fn new(path: Option<&PathBuf>, text: &[String]) -> Input {
        match path {
            Some(path) if has_extension(path, EXTENSION) => Input::Project(path.clone()),
            Some(path) => Input::File(path.clone()),
            None => Input::Text(text.to_vec()),
        }
    }

    /// The file it names, which outputs must not replace.
    pub(crate) fn path(&self) -> Option<&Path> {
        match self {
            Input::Project(path) | Input::File(path) => Some(path),
            Input::Text(_) => None,
        }
    }

    /// The project to start from. A picture's or drawing's bytes are left empty, to be
    /// read by [`load`] after the settings have been checked.
    pub(crate) fn project(&self) -> Result<Project, String> {
        Ok(match self {
            Input::Project(path) => read_project(&read(path)?).map_err(|e| format!("{}: {e}", path.display()))?,
            Input::File(path) => {
                let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                if has_extension(path, "svg") {
                    Project::new(Source::Svg { name, data: Vec::new() })
                } else {
                    Project::new(Source::Picture { name, data: Vec::new() })
                }
            }
            Input::Text(lines) => Project::new(Source::text(lines.join("\n"))),
        })
    }

    /// Reads a picture's or drawing's bytes into the project.
    pub(crate) fn load(&self, project: &mut Project) -> Result<(), String> {
        if let (Input::File(path), Source::Picture { data, .. } | Source::Svg { data, .. }) =
            (self, &mut project.source)
        {
            *data = read(path)?;
        }
        Ok(())
    }
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

pub(crate) fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Fonts given on the command line, embedded in the project so that it opens anywhere.
pub(crate) fn embed_fonts(paths: &[PathBuf]) -> Result<Vec<Font>, String> {
    paths
        .iter()
        .map(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            Ok(Font { name, data: Some(read(path)?) })
        })
        .collect()
}

/// The project's font files, the built-in one among them where it is named.
pub(crate) fn font_data(project: &Project) -> Result<Vec<&[u8]>, String> {
    project.font_data(BUILT_IN).map_err(|e| format!("{e} Give it with --font."))
}
