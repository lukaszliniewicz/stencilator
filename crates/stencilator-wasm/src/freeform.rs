//! Free-form mode for the browser, and projects for both modes.
//!
//! A [`Project`] holds the artwork, its fonts and every setting, which travel to and from
//! JavaScript as JSON in the project file's own form. An [`Artwork`] is the project's
//! artwork decoded or laid out once; it labels cells with paints ([`Labels`]), and the
//! labels make [`Plates`], which give the report and the files. Each step can be repeated
//! without the ones before it: ticking a paint off needs new plates, not new labels, and
//! merging paints needs new labels, which the artwork makes from the cells it labelled
//! last when only the merges have changed.

use stencilator_core::artwork::{ArtworkSettings, LabelMap, label_drawing, label_picture};
use stencilator_core::drawing::{Drawing, parse_svg, text_drawing};
use stencilator_core::plate::{Plate, PlateReport, build_plates, check_plate, plate_files, plates_3mf, reports_json};
use stencilator_core::project::{self, Colour, Font, Source, read_project};
use stencilator_core::source::{Image, decode};
use stencilator_core::zip::zip;
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use crate::{error, rgb};

/// A project: the artwork, its fonts and the settings of both modes.
#[wasm_bindgen]
pub struct Project {
    project: project::Project,
}

#[wasm_bindgen]
impl Project {
    /// Opens a project file.
    ///
    /// # Errors
    ///
    /// When it is not a project, comes from a newer version, or a setting is out of range.
    pub fn open(bytes: &[u8]) -> Result<Project, JsError> {
        Ok(Project { project: read_project(bytes).map_err(|e| error(&e))? })
    }

    /// A new project of a PNG, JPEG or WebP picture, in tile mode.
    #[must_use]
    pub fn picture(name: String, bytes: Vec<u8>) -> Project {
        Project { project: project::Project::new(Source::Picture { name, data: bytes }) }
    }

    /// A new project of an SVG drawing, in free-form mode at the drawing's own size.
    #[must_use]
    pub fn svg(name: String, bytes: Vec<u8>) -> Project {
        Project { project: project::Project::new(Source::Svg { name, data: bytes }) }
    }

    /// A new project of typed text, in black at the usual size, in free-form mode.
    #[must_use]
    pub fn text(text: String) -> Project {
        Project { project: project::Project::new(Source::text(text)) }
    }

    /// Changes typed text: the words, with lines separated by line breaks, the size of
    /// the font in millimetres, and the colour as `0xRRGGBB`.
    ///
    /// # Errors
    ///
    /// When the project's artwork is not typed text.
    #[wasm_bindgen(js_name = setText)]
    pub fn set_text(&mut self, text: String, font_mm: f64, colour: u32) -> Result<(), JsError> {
        match &mut self.project.source {
            Source::Text { .. } => {
                self.project.source = Source::Text { text, font_mm, colour: Colour(rgb(colour)) };
                Ok(())
            }
            _ => Err(JsError::new("Only typed text can be changed this way.")),
        }
    }

    /// Adds a font to the project, which then carries it. The first font sets typed text
    /// and stands in for any font a drawing names but the project lacks.
    #[wasm_bindgen(js_name = addFont)]
    pub fn add_font(&mut self, name: String, bytes: Vec<u8>) {
        self.project.fonts.push(Font { name, data: Some(bytes) });
    }

    /// Removes every font, so that the built-in one is used.
    #[wasm_bindgen(js_name = clearFonts)]
    pub fn clear_fonts(&mut self) {
        self.project.fonts.clear();
    }

    /// The settings as JSON, in the form of a project file without the artwork and fonts.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn settings(&self) -> String {
        String::from_utf8(self.project.settings_json()).unwrap_or_default()
    }

    /// Takes settings as JSON in the form [`Project::settings`] gives; fields left out
    /// take the engine's defaults.
    ///
    /// # Errors
    ///
    /// When the JSON cannot be read or a setting is out of range; nothing changes then.
    #[wasm_bindgen(js_name = setSettings)]
    pub fn set_settings(&mut self, json: &str) -> Result<(), JsError> {
        self.project.set_settings_json(json.as_bytes()).map_err(|e| error(&e))
    }

    /// The project file.
    #[must_use]
    pub fn file(&self) -> Vec<u8> {
        self.project.to_json()
    }
}

/// The artwork as the engine works with it.
enum Art {
    Picture(Image),
    Drawing(Box<Drawing>),
}

/// A project's artwork, decoded or laid out once, ready to label at any settings.
#[wasm_bindgen]
pub struct Artwork {
    art: Art,
    /// The settings last labelled at, and the cells they gave before merging.
    last: Option<(ArtworkSettings, LabelMap)>,
}

#[wasm_bindgen]
impl Artwork {
    /// Decodes the project's picture, or lays out its drawing or text. `built_in_font` is
    /// the file of [`stencilator_core::project::BUILT_IN_FONT`], which text may need.
    ///
    /// # Errors
    ///
    /// When the artwork cannot be read or laid out, or needs a font the project lacks.
    #[wasm_bindgen(constructor)]
    pub fn new(project: &Project, built_in_font: &[u8]) -> Result<Artwork, JsError> {
        let project = &project.project;
        let art = match &project.source {
            Source::Picture { data, .. } => Art::Picture(decode(data).map_err(|e| error(&e))?),
            Source::Svg { data, .. } => {
                let fonts = project.font_data(built_in_font).map_err(|e| error(&e))?;
                Art::Drawing(Box::new(parse_svg(data, &fonts).map_err(|e| error(&e))?))
            }
            Source::Text { text, font_mm, colour } => {
                let fonts = project.font_data(built_in_font).map_err(|e| error(&e))?;
                Art::Drawing(Box::new(text_drawing(text, &fonts, *font_mm, colour.0).map_err(|e| error(&e))?))
            }
        };
        Ok(Artwork { art, last: None })
    }

    /// The paint of every cell at the project's free-form and colour settings, with its
    /// merges applied.
    ///
    /// # Errors
    ///
    /// When a setting is out of range or the artwork would make too many cells.
    pub fn labels(&mut self, project: &Project) -> Result<Labels, JsError> {
        let settings = project.project.artwork_settings();
        let map = match self.last.take() {
            Some((last, map)) if last == settings => map,
            _ => match &self.art {
                Art::Picture(image) => label_picture(image, &settings),
                Art::Drawing(drawing) => label_drawing(drawing, &settings),
            }
            .map_err(|e| error(&e))?,
        };
        let labels = Labels { map: map.merged(&project.project.merges()) };
        self.last = Some((settings, map));
        Ok(labels)
    }
}

/// Which paint each cell of the artwork holds.
#[wasm_bindgen]
pub struct Labels {
    map: LabelMap,
}

#[wasm_bindgen]
impl Labels {
    /// Cells across.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn width(&self) -> u32 {
        self.map.width
    }

    /// Cells down.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn height(&self) -> u32 {
        self.map.height
    }

    /// Side of a cell in millimetres.
    #[wasm_bindgen(getter, js_name = cellMm)]
    #[must_use]
    pub fn cell_mm(&self) -> f64 {
        f64::from(self.map.cell_um) / 1000.0
    }

    /// Width of the artwork in millimetres.
    #[wasm_bindgen(getter, js_name = widthMm)]
    #[must_use]
    pub fn width_mm(&self) -> f64 {
        self.map.width_mm()
    }

    /// Height of the artwork in millimetres.
    #[wasm_bindgen(getter, js_name = heightMm)]
    #[must_use]
    pub fn height_mm(&self) -> f64 {
        self.map.height_mm()
    }

    /// The paints as RGB triples, darkest first.
    #[must_use]
    pub fn palette(&self) -> Vec<u8> {
        self.map.palette.concat()
    }

    /// The paint of every cell, rows from the top.
    #[must_use]
    pub fn labels(&self) -> Vec<u8> {
        self.map.labels.clone()
    }

    /// Number of cells of each paint.
    #[wasm_bindgen(js_name = cellCounts)]
    #[must_use]
    pub fn cell_counts(&self) -> Vec<u32> {
        self.map.cell_counts().into_iter().map(|n| u32::try_from(n).unwrap_or(u32::MAX)).collect()
    }

    /// One plate for every paint the project does not skip, each checked for what may not
    /// print well.
    ///
    /// # Errors
    ///
    /// When a plate setting is out of range or every paint is skipped.
    pub fn plates(&self, project: &Project) -> Result<Plates, JsError> {
        let (settings, skip) = (project.project.plate_settings(), project.project.stencil_skip());
        let plates = build_plates(&self.map, &settings, &skip).map_err(|e| error(&e))?;
        let reports = plates
            .iter()
            .map(|plate| check_plate(plate, &settings))
            .collect::<Result<_, _>>()
            .map_err(|e| error(&e))?;
        Ok(Plates { plates, reports })
    }
}

/// The plates of a free-form project, in palette order, with their reports.
#[wasm_bindgen]
pub struct Plates {
    plates: Vec<Plate>,
    reports: Vec<PlateReport>,
}

#[wasm_bindgen]
impl Plates {
    /// Number of plates.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn count(&self) -> u32 {
        u32::try_from(self.plates.len()).unwrap_or(u32::MAX)
    }

    /// Each plate's STL file name.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.plates.iter().map(Plate::file_name).collect()
    }

    /// What may not print well on each plate, as the JSON array the command line prints
    /// with `--json`: islands, thin webs and narrow openings, each placed in millimetres
    /// from the plate's bottom-left corner.
    #[must_use]
    pub fn report(&self) -> String {
        reports_json(&self.plates, &self.reports).to_string()
    }

    /// One plate as binary STL, at the project's thickness.
    ///
    /// # Errors
    ///
    /// When there is no such plate or the thickness is out of range.
    pub fn stl(&self, index: u32, project: &Project) -> Result<Vec<u8>, JsError> {
        let plate = usize::try_from(index).ok().and_then(|i| self.plates.get(i));
        let plate = plate.ok_or_else(|| JsError::new("There is no such plate."))?;
        plate.stl(project.project.stencils.thickness_mm).map_err(|e| error(&e))
    }

    /// Every plate as STL files in one zip.
    ///
    /// # Errors
    ///
    /// When the thickness is out of range.
    pub fn zip(&self, project: &Project) -> Result<Vec<u8>, JsError> {
        let files = plate_files(&self.plates, project.project.stencils.thickness_mm).map_err(|e| error(&e))?;
        zip(&files).map_err(|e| error(&e))
    }

    /// Every plate in one 3MF file, side by side, each named and in its paint's colour.
    ///
    /// # Errors
    ///
    /// When the thickness is out of range.
    #[wasm_bindgen(js_name = threeMf)]
    pub fn three_mf(&self, project: &Project) -> Result<Vec<u8>, JsError> {
        plates_3mf(&self.plates, project.project.stencils.thickness_mm).map_err(|e| error(&e))
    }
}
