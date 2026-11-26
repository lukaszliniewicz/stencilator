//! Where a project comes from: a project file, a picture, an SVG drawing or typed text,
//! and the fonts its text needs.

use std::path::{Path, PathBuf};

use stencilator_core::project::{Colour, EXTENSION, Font, Project, Source, read_project};

/// The fonts built into the command, by file name; the first stands in for any missing.
const BUNDLED: &[(&str, &[u8])] = &[("NotoSans-Bold.ttf", include_bytes!("../../../fonts/NotoSans-Bold.ttf"))];

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
            Input::Text(lines) => {
                Project::new(Source::Text { text: lines.join("\n"), font_mm: 30.0, colour: Colour([0, 0, 0]) })
            }
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

/// The project names the built-in font when its text needs one and it names none.
pub(crate) fn name_default_font(project: &mut Project) {
    let drawn = matches!(project.source, Source::Svg { .. } | Source::Text { .. });
    if drawn && project.fonts.is_empty() {
        project.fonts.push(Font { name: BUNDLED[0].0.to_owned(), data: None });
    }
}

/// The bytes of the project's fonts, the built-in one when it names none.
pub(crate) fn font_bytes(project: &Project) -> Result<Vec<Vec<u8>>, String> {
    if project.fonts.is_empty() {
        return Ok(vec![BUNDLED[0].1.to_vec()]);
    }
    project
        .fonts
        .iter()
        .map(|font| match &font.data {
            Some(data) => Ok(data.clone()),
            None => {
                BUNDLED.iter().find(|(name, _)| *name == font.name).map(|(_, data)| data.to_vec()).ok_or_else(|| {
                    format!("the project uses the font {}, which is not built in; give it with --font", font.name)
                })
            }
        })
        .collect()
}
