//! Free-form mode for the browser, and projects for both modes.
//!
//! A [`Project`] holds the artwork, its fonts and every setting, which travel to and from
//! JavaScript as JSON in the project file's own form. An [`Artwork`] is the project's
//! artwork decoded or laid out once; it labels cells with paints ([`Labels`]), and the
//! labels make [`Plates`], which give the report and the files. Each step can be repeated
//! without the ones before it: ticking a paint off needs new plates, not new labels, and
//! merging paints needs new labels, which the artwork makes from the cells it labelled
//! last when only the merges have changed.

use std::rc::Rc;

use stencilator_core::artwork::{ArtworkSettings, LabelMap, label_drawing, label_picture};
use stencilator_core::chart::{build_chart, merge_paints};
use stencilator_core::drawing::{Drawing, parse_svg, text_drawing};
use stencilator_core::plate::{
    Plate, PlateProblems, PlateReport, build_plates, inspect_plate, plate_drawing, plate_files, plates_3mf,
    reports_json,
};
use stencilator_core::project::{self, BUILT_IN_FONTS, Colour, Font, Source, read_project};
use stencilator_core::source::{Image, decode};
use stencilator_core::zip::zip;
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use crate::{Chart, Source as Picture, error, rgb};

/// Keep previews small even when the artwork has millions of cells.
const PREVIEW_SIDE: f64 = 800.0;

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

    /// Names a built-in font, which the project file then names without carrying it.
    ///
    /// # Errors
    ///
    /// When no font of that file name is built in.
    #[wasm_bindgen(js_name = addBuiltInFont)]
    pub fn add_built_in_font(&mut self, name: String) -> Result<(), JsError> {
        if !BUILT_IN_FONTS.contains(&name.as_str()) {
            return Err(JsError::new("There is no built-in font of that name."));
        }
        self.project.fonts.push(Font { name, data: None });
        Ok(())
    }

    /// Removes every font, so that the default built-in one is used.
    #[wasm_bindgen(js_name = clearFonts)]
    pub fn clear_fonts(&mut self) {
        self.project.fonts.clear();
    }

    /// The file names of the project's fonts, in order.
    #[wasm_bindgen(js_name = fontNames)]
    #[must_use]
    pub fn font_names(&self) -> Vec<String> {
        self.project.fonts.iter().map(|font| font.name.clone()).collect()
    }

    /// The bytes of an embedded font, or nothing for a built-in font.
    #[wasm_bindgen(js_name = fontBytes)]
    #[must_use]
    pub fn font_bytes(&self, index: usize) -> Option<Vec<u8>> {
        self.project.fonts.get(index).and_then(|font| font.data.clone())
    }

    /// What the artwork is: `picture`, `svg` or `text`.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn kind(&self) -> String {
        match self.project.source {
            Source::Picture { .. } => "picture",
            Source::Svg { .. } => "svg",
            Source::Text { .. } => "text",
        }
        .to_owned()
    }

    /// The file name of a picture or drawing, or nothing for typed text.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn name(&self) -> String {
        match &self.project.source {
            Source::Picture { name, .. } | Source::Svg { name, .. } => name.clone(),
            Source::Text { .. } => String::new(),
        }
    }

    /// The picture or drawing file itself; nothing for typed text.
    #[wasm_bindgen(js_name = sourceData)]
    #[must_use]
    pub fn source_data(&self) -> Vec<u8> {
        match &self.project.source {
            Source::Picture { data, .. } | Source::Svg { data, .. } => data.clone(),
            Source::Text { .. } => Vec::new(),
        }
    }

    /// Typed text, lines separated by line breaks; nothing for other artwork.
    #[wasm_bindgen(getter, js_name = typedText)]
    #[must_use]
    pub fn typed_text(&self) -> String {
        match &self.project.source {
            Source::Text { text, .. } => text.clone(),
            _ => String::new(),
        }
    }

    /// The size of typed text's font in millimetres, or the usual size for other artwork.
    #[wasm_bindgen(getter, js_name = fontMm)]
    #[must_use]
    pub fn font_mm(&self) -> f64 {
        match &self.project.source {
            Source::Text { font_mm, .. } => *font_mm,
            _ => project::TEXT_MM,
        }
    }

    /// The colour of typed text as `0xRRGGBB`, or black for other artwork.
    #[wasm_bindgen(getter, js_name = textColour)]
    #[must_use]
    pub fn text_colour(&self) -> u32 {
        match &self.project.source {
            Source::Text { colour: Colour([r, g, b]), .. } => u32::from_be_bytes([0, *r, *g, *b]),
            _ => 0,
        }
    }

    /// The settings as JSON, in the form of a project file without the artwork and fonts.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn settings(&self) -> String {
        String::from_utf8(self.project.settings_json()).unwrap_or_default()
    }

    /// Takes settings as JSON in the form [`Project::settings`] gives; fields left out
    /// take the engine's defaults. `keep_seed` preserves the exact seed from the project
    /// file when the interface changes settings without editing the seed.
    ///
    /// # Errors
    ///
    /// When the JSON cannot be read or a setting is out of range; nothing changes then.
    #[wasm_bindgen(js_name = setSettings)]
    pub fn set_settings(&mut self, json: &str, keep_seed: Option<bool>) -> Result<(), JsError> {
        let seed = self.project.colours.seed;
        self.project.set_settings_json(json.as_bytes()).map_err(|e| error(&e))?;
        if keep_seed == Some(true) {
            self.project.colours.seed = seed;
        }
        Ok(())
    }

    /// Builds a tile chart from the decoded picture and the project's settings, including
    /// a seed that may be too large to represent as a JavaScript number.
    ///
    /// # Errors
    ///
    /// When the project does not contain a picture or the settings cannot make a chart.
    #[wasm_bindgen(js_name = tileChart)]
    pub fn tile_chart(&self, picture: &Picture) -> Result<Chart, JsError> {
        if !matches!(self.project.source, Source::Picture { .. }) {
            return Err(JsError::new("Tile mode needs a PNG, JPEG or WebP picture."));
        }
        let chart = build_chart(&picture.image, &self.project.chart_settings()).map_err(|e| error(&e))?;
        Ok(Chart { chart: merge_paints(&chart, &self.project.merges()) })
    }

    /// The project file.
    #[must_use]
    pub fn file(&self) -> Vec<u8> {
        self.project.to_json()
    }
}

/// The fonts a shell builds in, by file name, for [`Artwork`] to set text in.
#[wasm_bindgen]
#[derive(Default)]
pub struct Fonts {
    fonts: Vec<(String, Vec<u8>)>,
}

#[wasm_bindgen]
impl Fonts {
    /// No fonts yet.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Fonts {
        Fonts::default()
    }

    /// The file names of the fonts every shell builds in, the default first.
    #[wasm_bindgen(js_name = builtIn)]
    #[must_use]
    pub fn built_in() -> Vec<String> {
        BUILT_IN_FONTS.iter().map(|&name| name.to_owned()).collect()
    }

    /// Adds a built-in font's file under its file name.
    pub fn add(&mut self, name: String, bytes: Vec<u8>) {
        self.fonts.push((name, bytes));
    }
}

/// The artwork as the engine works with it.
enum Art {
    Picture(Rc<Image>),
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
    /// Decodes the project's picture, or lays out its drawing or text, in the project's
    /// fonts and the built-in ones in `fonts`, which must hold at least the default.
    ///
    /// # Errors
    ///
    /// When the artwork cannot be read or laid out, or needs a font that is missing.
    #[wasm_bindgen(constructor)]
    pub fn new(project: &Project, fonts: &Fonts) -> Result<Artwork, JsError> {
        let project = &project.project;
        let built_in: Vec<(&str, &[u8])> =
            fonts.fonts.iter().map(|(name, data)| (name.as_str(), data.as_slice())).collect();
        let art = match &project.source {
            Source::Picture { data, .. } => Art::Picture(Rc::new(decode(data).map_err(|e| error(&e))?)),
            Source::Svg { data, .. } => {
                let fonts = project.font_data(&built_in).map_err(|e| error(&e))?;
                Art::Drawing(Box::new(parse_svg(data, &fonts).map_err(|e| error(&e))?))
            }
            Source::Text { text, font_mm, colour } => {
                let fonts = project.font_data(&built_in).map_err(|e| error(&e))?;
                Art::Drawing(Box::new(text_drawing(text, &fonts, *font_mm, colour.0).map_err(|e| error(&e))?))
            }
        };
        Ok(Artwork { art, last: None })
    }

    /// Shares the pixels already decoded for tile mode.
    #[wasm_bindgen(js_name = fromPicture)]
    #[must_use]
    pub fn from_picture(picture: &Picture) -> Artwork {
        Artwork { art: Art::Picture(Rc::clone(&picture.image)), last: None }
    }

    /// A bounded RGBA preview of a drawing or typed text, made with the engine's fonts.
    /// Pictures can be shown directly from the original file.
    ///
    /// # Errors
    ///
    /// When the drawing cannot be rendered.
    pub fn preview(&self) -> Result<Option<RasterPreview>, JsError> {
        let Art::Drawing(drawing) = &self.art else { return Ok(None) };
        let (w, h) = drawing.size_mm();
        let (width, height) = preview_size(w, h, PREVIEW_SIDE);
        drawing.render(width, height).map(RasterPreview).map(Some).map_err(|e| error(&e))
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

    /// A bounded RGBA preview sampled from the actual paint cells.
    #[must_use]
    pub fn preview(&self) -> RasterPreview {
        let (full_width, full_height) = (f64::from(self.map.width), f64::from(self.map.height));
        let longest = full_width.max(full_height).min(PREVIEW_SIDE);
        let (width, height) = preview_size(full_width, full_height, longest);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        let sample = |at, full, preview| u64::from(at * 2 + 1) * u64::from(full) / u64::from(preview * 2);
        for y in 0..height {
            let row = sample(y, self.map.height, height) * u64::from(self.map.width);
            for x in 0..width {
                let i = usize::try_from(row + sample(x, self.map.width, width)).unwrap_or_default();
                let paint = usize::from(self.map.labels[i]);
                rgba.extend_from_slice(&self.map.palette[paint]);
                rgba.push(255);
            }
        }
        RasterPreview(Image { width, height, rgba })
    }

    /// One plate for every paint the project does not skip, each checked for what may not
    /// print well.
    ///
    /// # Errors
    ///
    /// When a plate setting is out of range. If every paint is skipped, the workspace
    /// can still show and edit the paints, with an empty list of plates.
    pub fn plates(&self, project: &Project) -> Result<Plates, JsError> {
        let (settings, skip) = (project.project.plate_settings(), project.project.stencil_skip());
        if self.map.palette.iter().all(|paint| skip.contains(paint)) {
            return Ok(Plates { built: Vec::new(), reports: Vec::new(), problems: Vec::new() });
        }
        let plates = build_plates(&self.map, &settings, &skip).map_err(|e| error(&e))?;
        let (reports, problems) = plates
            .iter()
            .map(|plate| inspect_plate(plate, &settings))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| error(&e))?
            .into_iter()
            .unzip();
        Ok(Plates { built: plates, reports, problems })
    }
}

/// An image small enough to send to the interface, in straight RGBA.
#[wasm_bindgen]
pub struct RasterPreview(Image);

#[wasm_bindgen]
impl RasterPreview {
    /// Width in pixels.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn width(&self) -> u32 {
        self.0.width
    }

    /// Height in pixels.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn height(&self) -> u32 {
        self.0.height
    }

    /// Hands over the pixels and consumes the preview.
    #[must_use]
    pub fn pixels(self) -> Vec<u8> {
        self.0.rgba
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "preview sides are in 1..=800")]
fn preview_size(width: f64, height: f64, longest: f64) -> (u32, u32) {
    let scale = longest / width.max(height);
    let side = |size: f64| (size * scale).round().max(1.0) as u32;
    (side(width), side(height))
}

/// The plates of a free-form project, in palette order, with their reports.
#[wasm_bindgen]
pub struct Plates {
    built: Vec<Plate>,
    reports: Vec<PlateReport>,
    problems: Vec<PlateProblems>,
}

#[wasm_bindgen]
impl Plates {
    /// Number of plates.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn count(&self) -> u32 {
        u32::try_from(self.built.len()).unwrap_or(u32::MAX)
    }

    /// Each plate's STL file name.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.built.iter().map(Plate::file_name).collect()
    }

    /// What may not print well on each plate, as the JSON array the command line prints
    /// with `--json`: islands, thin webs and narrow openings, each placed in millimetres
    /// from the plate's bottom-left corner.
    #[must_use]
    pub fn report(&self) -> String {
        reports_json(&self.built, &self.reports).to_string()
    }

    /// One plate as a page draws it, as JSON: its size, and SVG path data in millimetres,
    /// y down, for its material, islands, thin webs and narrow openings.
    ///
    /// # Errors
    ///
    /// When there is no such plate.
    pub fn drawing(&self, index: u32) -> Result<String, JsError> {
        let i = usize::try_from(index).map_err(|_| JsError::new("There is no such plate."))?;
        let (Some(plate), Some(problems)) = (self.built.get(i), self.problems.get(i)) else {
            return Err(JsError::new("There is no such plate."));
        };
        Ok(plate_drawing(plate, problems).to_json())
    }

    /// One plate as binary STL, at the project's thickness.
    ///
    /// # Errors
    ///
    /// When there is no such plate or the thickness is out of range.
    pub fn stl(&self, index: u32, project: &Project) -> Result<Vec<u8>, JsError> {
        let plate = usize::try_from(index).ok().and_then(|i| self.built.get(i));
        let plate = plate.ok_or_else(|| JsError::new("There is no such plate."))?;
        plate.stl(project.project.stencils.thickness_mm).map_err(|e| error(&e))
    }

    /// Every plate as STL files in one zip.
    ///
    /// # Errors
    ///
    /// When the thickness is out of range.
    pub fn zip(&self, project: &Project) -> Result<Vec<u8>, JsError> {
        let files = plate_files(&self.built, project.project.stencils.thickness_mm).map_err(|e| error(&e))?;
        zip(&files).map_err(|e| error(&e))
    }

    /// Every plate in one 3MF file, side by side, each named and in its paint's colour.
    ///
    /// # Errors
    ///
    /// When the thickness is out of range.
    #[wasm_bindgen(js_name = threeMf)]
    pub fn three_mf(&self, project: &Project) -> Result<Vec<u8>, JsError> {
        plates_3mf(&self.built, project.project.stencils.thickness_mm).map_err(|e| error(&e))
    }
}
