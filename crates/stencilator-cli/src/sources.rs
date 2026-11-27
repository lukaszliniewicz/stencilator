//! Where a project comes from: a project file, a picture, an SVG drawing or typed text,
//! and the fonts its text needs.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use stencilator_core::project::{BUILT_IN_FONT, EXTENSION, Font, Project, STENCIL_FONT, Source, read_project};

/// The built-in fonts: their names on the command line, their file names in projects, and
/// the files themselves.
const BUILT_IN: [(&str, &str, &[u8]); 2] = [
    ("Noto Sans Bold", BUILT_IN_FONT, include_bytes!("../../../fonts/NotoSans-Bold.ttf")),
    ("Allerta Stencil", STENCIL_FONT, include_bytes!("../../../fonts/AllertaStencil-Regular.ttf")),
];

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
            Some(path) if has_extension(path, EXTENSION) || starts_with_json_object(path) => {
                Input::Project(path.clone())
            }
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
            Input::File(path) => artwork(path, Vec::new()),
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

/// A new project from a picture or SVG, named after its source file.
pub(crate) fn artwork(path: &Path, data: Vec<u8>) -> Project {
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let source = if has_extension(path, "svg") { Source::Svg { name, data } } else { Source::Picture { name, data } };
    Project::new(source)
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

/// The CLI has always let `--save` use any name. A JSON object starts differently from
/// every supported image; `read_project` checks its marker and version.
fn starts_with_json_object(path: &Path) -> bool {
    // A pipe must only be read once, by the artwork loader.
    if !path.is_file() {
        return false;
    }
    let mut prefix = [0; 256];
    File::open(path)
        .and_then(|mut file| file.read(&mut prefix))
        .is_ok_and(|length| prefix[..length].iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'{'))
}

pub(crate) fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Fonts given on the command line: a built-in one by its name, which the project names,
/// or a font file, which the project carries so that it opens anywhere.
pub(crate) fn embed_fonts(fonts: &[PathBuf]) -> Result<Vec<Font>, String> {
    fonts
        .iter()
        .map(|font| {
            let given = font.to_string_lossy();
            if let Some((_, file, _)) = BUILT_IN.iter().find(|(name, ..)| name.eq_ignore_ascii_case(&given)) {
                return Ok(Font { name: (*file).to_owned(), data: None });
            }
            let name = font.file_name().unwrap_or_default().to_string_lossy().into_owned();
            Ok(Font { name, data: Some(read(font)?) })
        })
        .collect()
}

/// The project's font files, the built-in one among them where it is named.
pub(crate) fn font_data(project: &Project) -> Result<Vec<&[u8]>, String> {
    let built_in: Vec<(&str, &[u8])> = BUILT_IN.iter().map(|&(_, file, data)| (file, data)).collect();
    project.font_data(&built_in).map_err(|e| format!("{e} Give it with --font."))
}
