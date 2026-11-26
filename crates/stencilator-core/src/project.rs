//! Project files: everything needed to make the same stencils again, in one JSON file.
//!
//! The artwork is embedded, with any fonts it needs that Stencilator does not bundle, next
//! to the settings of both modes and the paint choices, which name paints by colour. Any
//! field but the source may be left out and takes the engine's default, so a short
//! hand-written project opens too. A drawing or typed text defaults to free-form mode at
//! its own size; a picture to tile mode, and to 200 mm wide in free-form mode. The field `stencilator` holds the format's version and
//! marks the file as a project; files use the extension `.stencilator`.
//!
//! ```json
//! {
//!   "stencilator": 1,
//!   "mode": "freeform",
//!   "source": { "kind": "text", "text": "Hello", "font_mm": 30, "colour": "#1d2f5c" },
//!   "fonts": [{ "name": "NotoSans-Bold.ttf" }],
//!   "freeform": { "size": "natural", "cell_mm": 0.1 }
//! }
//! ```

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::artwork::{ArtworkSettings, Size};
use crate::chart::{ChartSettings, Sizing};
use crate::colour::Rgb;
use crate::plate::PlateSettings;
use crate::quantise::ColourMode;
use crate::{Error, VERSION};

/// The version of the format this engine writes, and the newest it reads.
pub const FORMAT: u32 = 1;

/// The extension of project files, without the dot.
pub const EXTENSION: &str = "stencilator";

/// A project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// The format's version.
    #[serde(default = "format")]
    pub stencilator: u32,
    /// The version of Stencilator that wrote the file, for information.
    #[serde(default)]
    pub written_by: String,
    /// Which kind of stencils the project makes.
    #[serde(default)]
    pub mode: Mode,
    /// The artwork.
    pub source: Source,
    /// Fonts for text, the first standing in for any the artwork names but lacks.
    #[serde(default)]
    pub fonts: Vec<Font>,
    /// How colours become paints, in both modes.
    #[serde(default)]
    pub colours: Colours,
    /// Tile mode.
    #[serde(default)]
    pub tiles: Tiles,
    /// Free-form mode.
    #[serde(default)]
    pub freeform: Freeform,
    /// Plates and the choices made about paints, in both modes.
    #[serde(default)]
    pub stencils: Stencils,
}

fn format() -> u32 {
    FORMAT
}

/// Which kind of stencils a project makes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// A grid of square tiles.
    #[default]
    Tiles,
    /// Outlines that follow the artwork.
    Freeform,
}

/// The artwork.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    /// A PNG, JPEG or WebP picture.
    Picture {
        /// Its file name.
        #[serde(default)]
        name: String,
        /// The file itself, in base64.
        #[serde(with = "bytes")]
        data: Vec<u8>,
    },
    /// An SVG drawing.
    Svg {
        /// Its file name.
        #[serde(default)]
        name: String,
        /// The file itself, in base64.
        #[serde(with = "bytes")]
        data: Vec<u8>,
    },
    /// Typed text in one colour, set in the first font.
    Text {
        /// The text; lines are separated by line breaks.
        text: String,
        /// The size of the font in millimetres.
        font_mm: f64,
        /// The colour of the letters.
        colour: Colour,
    },
}

/// A font.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Font {
    /// Its file name. Without `data`, the font Stencilator bundles under this name.
    pub name: String,
    /// The font file itself, in base64, when it is not one Stencilator bundles.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_bytes")]
    pub data: Option<Vec<u8>>,
}

/// A colour, written `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour(pub Rgb);

impl Serialize for Colour {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let [r, g, b] = self.0;
        serializer.serialize_str(&format!("#{r:02x}{g:02x}{b:02x}"))
    }
}

impl<'de> Deserialize<'de> for Colour {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let digits = text.strip_prefix('#').filter(|d| d.len() == 6 && d.is_ascii());
        let byte = |i: usize| digits.and_then(|d| u8::from_str_radix(&d[i..i + 2], 16).ok());
        match (byte(0), byte(2), byte(4)) {
            (Some(r), Some(g), Some(b)) => Ok(Colour([r, g, b])),
            _ => Err(D::Error::custom(format!("{text:?} is not a colour written #rrggbb"))),
        }
    }
}

/// How colours become paints.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colours {
    /// Colour, greyscale or black and white.
    pub mode: Palette,
    /// Most paints.
    pub count: usize,
    /// The colour of the canvas, which transparent parts take.
    pub background: Colour,
    /// Seed for the colour clustering.
    pub seed: u64,
}

impl Default for Colours {
    fn default() -> Self {
        let engine = ChartSettings::default();
        Colours {
            mode: engine.mode.into(),
            count: engine.n_colours,
            background: Colour(engine.background),
            seed: engine.seed,
        }
    }
}

/// How colours are reduced, as the file names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Palette {
    /// Colours.
    Colour,
    /// Shades of grey.
    Greyscale,
    /// Black and white.
    BlackWhite,
}

impl From<ColourMode> for Palette {
    fn from(mode: ColourMode) -> Self {
        match mode {
            ColourMode::Colour => Palette::Colour,
            ColourMode::Greyscale => Palette::Greyscale,
            ColourMode::BlackWhite => Palette::BlackWhite,
        }
    }
}

impl From<Palette> for ColourMode {
    fn from(palette: Palette) -> Self {
        match palette {
            Palette::Colour => ColourMode::Colour,
            Palette::Greyscale => ColourMode::Greyscale,
            Palette::BlackWhite => ColourMode::BlackWhite,
        }
    }
}

/// Tile mode's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tiles {
    /// Whether the tiles fill the canvas or follow the picture's pixels.
    pub sizing: TileSizing,
    /// Width and height of the canvas in millimetres.
    pub canvas_mm: [f64; 2],
    /// Side of a tile in millimetres.
    pub tile_mm: f64,
    /// Width of a grid line in millimetres.
    pub line_mm: f64,
    /// Resolution of the chart in dots per inch.
    pub dpi: u32,
    /// Colour of the grid lines on the chart.
    pub grid_colour: Colour,
}

impl Default for Tiles {
    fn default() -> Self {
        Tiles::from(&ChartSettings::default())
    }
}

impl From<&ChartSettings> for Tiles {
    fn from(settings: &ChartSettings) -> Self {
        Tiles {
            sizing: settings.sizing.into(),
            canvas_mm: [settings.canvas_mm.0, settings.canvas_mm.1],
            tile_mm: settings.tile_mm,
            line_mm: settings.line_mm,
            dpi: settings.dpi,
            grid_colour: Colour([0, 0, 0]),
        }
    }
}

/// How the number of tiles is decided, as the file names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TileSizing {
    /// As many tiles as fit the canvas.
    Canvas,
    /// One tile per pixel.
    Pixels,
}

impl From<Sizing> for TileSizing {
    fn from(sizing: Sizing) -> Self {
        match sizing {
            Sizing::Canvas => TileSizing::Canvas,
            Sizing::Pixels => TileSizing::Pixels,
        }
    }
}

impl From<TileSizing> for Sizing {
    fn from(sizing: TileSizing) -> Self {
        match sizing {
            TileSizing::Canvas => Sizing::Canvas,
            TileSizing::Pixels => Sizing::Pixels,
        }
    }
}

/// Free-form mode's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Freeform {
    /// How large the artwork is made.
    pub size: ArtworkSize,
    /// Side of a cell in millimetres.
    pub cell_mm: f64,
    /// Patches smaller than this, in square millimetres, take the paints around them.
    pub min_patch_mm2: f64,
    /// Solid border around the artwork, in millimetres.
    pub margin_mm: f64,
    /// Narrowest plate material not reported, in millimetres.
    pub min_web_mm: f64,
    /// Narrowest opening not reported, in millimetres.
    pub min_opening_mm: f64,
}

impl Default for Freeform {
    fn default() -> Self {
        let (artwork, plates) = (ArtworkSettings::default(), PlateSettings::default());
        Freeform {
            size: artwork.size.into(),
            cell_mm: artwork.cell_mm,
            min_patch_mm2: artwork.min_patch_mm2,
            margin_mm: plates.margin_mm,
            min_web_mm: plates.min_web_mm,
            min_opening_mm: plates.min_opening_mm,
        }
    }
}

/// How large free-form artwork is made, as the file names it: `"natural"`,
/// `{"width_mm": 200}`, `{"height_mm": 120}` or `{"within_mm": [200, 120]}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkSize {
    /// The artwork's own size.
    Natural,
    /// This wide.
    WidthMm(f64),
    /// This tall.
    HeightMm(f64),
    /// As large as fits this box.
    WithinMm([f64; 2]),
}

impl From<Size> for ArtworkSize {
    fn from(size: Size) -> Self {
        match size {
            Size::Natural => ArtworkSize::Natural,
            Size::Width(w) => ArtworkSize::WidthMm(w),
            Size::Height(h) => ArtworkSize::HeightMm(h),
            Size::Within(w, h) => ArtworkSize::WithinMm([w, h]),
        }
    }
}

impl From<ArtworkSize> for Size {
    fn from(size: ArtworkSize) -> Self {
        match size {
            ArtworkSize::Natural => Size::Natural,
            ArtworkSize::WidthMm(w) => Size::Width(w),
            ArtworkSize::HeightMm(h) => Size::Height(h),
            ArtworkSize::WithinMm([w, h]) => Size::Within(w, h),
        }
    }
}

/// Plates and the choices made about paints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stencils {
    /// Thickness of the plates in millimetres.
    pub thickness_mm: f64,
    /// Width and depth of the print bed in millimetres; larger plates are reported.
    pub bed_mm: [f64; 2],
    /// Pairs of paints, applied in order: the first becomes the second.
    pub merges: Vec<[Colour; 2]>,
    /// Paints that get no stencil.
    pub skipped: Vec<Colour>,
    /// Whether a paint of the canvas colour gets a stencil. It rarely needs one.
    pub stencil_canvas_colour: bool,
}

impl Default for Stencils {
    fn default() -> Self {
        let plates = PlateSettings::default();
        Stencils {
            thickness_mm: plates.thickness_mm,
            bed_mm: [plates.bed_mm.0, plates.bed_mm.1],
            merges: Vec::new(),
            skipped: Vec::new(),
            stencil_canvas_colour: false,
        }
    }
}

impl Project {
    /// A project of `source` with every setting at the engine's default.
    pub fn new(source: Source) -> Project {
        let mut project = Project {
            stencilator: FORMAT,
            written_by: VERSION.to_owned(),
            mode: Mode::default(),
            source,
            fonts: Vec::new(),
            colours: Colours::default(),
            tiles: Tiles::default(),
            freeform: Freeform::default(),
            stencils: Stencils::default(),
        };
        if project.is_drawn() {
            project.mode = Mode::Freeform;
            project.freeform.size = ArtworkSize::Natural;
        }
        project
    }

    /// Whether the source is a drawing or text, which has a size of its own.
    fn is_drawn(&self) -> bool {
        matches!(self.source, Source::Svg { .. } | Source::Text { .. })
    }

    /// Tile mode's settings for the engine.
    pub fn chart_settings(&self) -> ChartSettings {
        let t = &self.tiles;
        ChartSettings {
            sizing: t.sizing.into(),
            canvas_mm: (t.canvas_mm[0], t.canvas_mm[1]),
            tile_mm: t.tile_mm,
            line_mm: t.line_mm,
            dpi: t.dpi,
            mode: self.colours.mode.into(),
            n_colours: self.colours.count,
            background: self.colours.background.0,
            seed: self.colours.seed,
        }
    }

    /// Takes tile mode's settings, and the colour settings both modes share.
    pub fn set_chart_settings(&mut self, settings: &ChartSettings) {
        self.tiles = Tiles { grid_colour: self.tiles.grid_colour, ..Tiles::from(settings) };
        self.colours = Colours {
            mode: settings.mode.into(),
            count: settings.n_colours,
            background: Colour(settings.background),
            seed: settings.seed,
        };
    }

    /// Free-form mode's artwork settings for the engine.
    pub fn artwork_settings(&self) -> ArtworkSettings {
        let f = &self.freeform;
        ArtworkSettings {
            size: f.size.into(),
            cell_mm: f.cell_mm,
            mode: self.colours.mode.into(),
            n_colours: self.colours.count,
            background: self.colours.background.0,
            min_patch_mm2: f.min_patch_mm2,
            seed: self.colours.seed,
        }
    }

    /// Takes free-form mode's artwork settings, and the colour settings both modes share.
    pub fn set_artwork_settings(&mut self, settings: &ArtworkSettings) {
        self.freeform.size = settings.size.into();
        self.freeform.cell_mm = settings.cell_mm;
        self.freeform.min_patch_mm2 = settings.min_patch_mm2;
        self.colours = Colours {
            mode: settings.mode.into(),
            count: settings.n_colours,
            background: Colour(settings.background),
            seed: settings.seed,
        };
    }

    /// Free-form mode's plate settings for the engine.
    pub fn plate_settings(&self) -> PlateSettings {
        let f = &self.freeform;
        PlateSettings {
            thickness_mm: self.stencils.thickness_mm,
            margin_mm: f.margin_mm,
            min_web_mm: f.min_web_mm,
            min_opening_mm: f.min_opening_mm,
            bed_mm: (self.stencils.bed_mm[0], self.stencils.bed_mm[1]),
        }
    }

    /// Takes free-form mode's plate settings, and the thickness and bed both modes share.
    pub fn set_plate_settings(&mut self, settings: &PlateSettings) {
        self.stencils.thickness_mm = settings.thickness_mm;
        self.stencils.bed_mm = [settings.bed_mm.0, settings.bed_mm.1];
        self.freeform.margin_mm = settings.margin_mm;
        self.freeform.min_web_mm = settings.min_web_mm;
        self.freeform.min_opening_mm = settings.min_opening_mm;
    }

    /// The merges, as the engine takes them.
    pub fn merges(&self) -> Vec<(Rgb, Rgb)> {
        self.stencils.merges.iter().map(|[from, to]| (from.0, to.0)).collect()
    }

    /// The paints ticked off, as the engine takes them.
    pub fn skipped(&self) -> Vec<Rgb> {
        self.stencils.skipped.iter().map(|c| c.0).collect()
    }

    /// The paints that get no stencil: those ticked off, and the canvas colour unless
    /// [`Stencils::stencil_canvas_colour`] says it gets one.
    pub fn stencil_skip(&self) -> Vec<Rgb> {
        let mut skip = self.skipped();
        let canvas = self.colours.background.0;
        if !self.stencils.stencil_canvas_colour && !skip.contains(&canvas) {
            skip.push(canvas);
        }
        skip
    }

    /// The project as a file: indented JSON, stamped with this format and this version of
    /// Stencilator. The same project always gives the same bytes.
    pub fn to_json(&self) -> Vec<u8> {
        let stamped = Project { stencilator: FORMAT, written_by: VERSION.to_owned(), ..self.clone() };
        let mut out = serde_json::to_vec_pretty(&stamped).unwrap_or_default();
        out.push(b'\n');
        out
    }
}

/// Reads a project file and checks its settings.
///
/// # Errors
///
/// [`Error::Project`] when it is not a project, comes from a newer version of
/// Stencilator, or cannot be read; otherwise the error of the first setting out of range.
pub fn read_project(bytes: &[u8]) -> Result<Project, Error> {
    let damaged = |e: serde_json::Error| Error::Project(format!("The project could not be read: {e}."));
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(damaged)?;
    match value.get("stencilator").and_then(serde_json::Value::as_u64) {
        Some(format) if (1..=u64::from(FORMAT)).contains(&format) => {}
        Some(format) if format > u64::from(FORMAT) => {
            return Err(Error::Project(format!(
                "This project was saved by a newer version of Stencilator (format {format}); this one \
                 opens format {FORMAT}. Update Stencilator to open it."
            )));
        }
        _ => return Err(Error::Project("This is not a Stencilator project.".to_owned())),
    }
    let (has_mode, has_size) = (value.get("mode").is_some(), value.pointer("/freeform/size").is_some());
    let mut project: Project = serde_json::from_value(value).map_err(damaged)?;
    if project.is_drawn() {
        if !has_mode {
            project.mode = Mode::Freeform;
        }
        if !has_size {
            project.freeform.size = ArtworkSize::Natural;
        }
    }
    project.chart_settings().check()?;
    project.artwork_settings().check()?;
    project.plate_settings().check()?;
    Ok(project)
}

mod bytes {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;

    use super::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(|_| serde::de::Error::custom("an embedded file is damaged"))
    }
}

mod optional_bytes {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;

    use super::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::ref_option, reason = "serde passes the field by reference")]
    pub(super) fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(bytes) => serializer.serialize_str(&STANDARD.encode(bytes)),
            None => serializer.serialize_none(),
        }
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error> {
        let text = Option::<String>::deserialize(deserializer)?;
        text.map(|t| STANDARD.decode(t).map_err(|_| serde::de::Error::custom("an embedded font is damaged")))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_source() -> Source {
        Source::Text { text: "Hi".to_owned(), font_mm: 20.0, colour: Colour([0, 0, 0]) }
    }

    #[test]
    fn a_short_project_takes_the_defaults() {
        let project = read_project(
            br##"{"stencilator": 1, "source": {"kind": "text", "text": "Hi", "font_mm": 20, "colour": "#000000"}}"##,
        )
        .unwrap();
        assert_eq!(project.source, text_source());
        // Text is cut free-form at its own size.
        assert_eq!(project.mode, Mode::Freeform);
        assert_eq!(project.chart_settings(), ChartSettings::default());
        assert_eq!(project.artwork_settings(), ArtworkSettings { size: Size::Natural, ..ArtworkSettings::default() });
        assert_eq!(project.plate_settings(), PlateSettings::default());
        assert_eq!(project.tiles.grid_colour, Colour([0, 0, 0]));
        assert!(project.merges().is_empty() && project.skipped().is_empty() && !project.stencils.stencil_canvas_colour);
        assert_eq!(project, Project { written_by: String::new(), ..Project::new(text_source()) });
        // The canvas colour gets no stencil unless asked for.
        assert_eq!(project.stencil_skip(), [[255, 255, 255]]);
        let mut cut = project.clone();
        cut.stencils.stencil_canvas_colour = true;
        assert_eq!(cut.stencil_skip(), Vec::<[u8; 3]>::new());
        // A picture is a tile project, 200 mm wide in free-form mode, unless it says otherwise.
        let picture = read_project(br#"{"stencilator": 1, "source": {"kind": "picture", "data": ""}}"#).unwrap();
        assert_eq!((picture.mode, picture.artwork_settings()), (Mode::Tiles, ArtworkSettings::default()));
        let said = br#"{"stencilator": 1, "mode": "tiles", "freeform": {"size": {"width_mm": 50}},
            "source": {"kind": "svg", "data": ""}}"#;
        let said = read_project(said).unwrap();
        assert_eq!((said.mode, said.artwork_settings().size), (Mode::Tiles, Size::Width(50.0)));
    }

    #[test]
    fn settings_go_in_and_come_back() {
        let mut project = Project::new(Source::Picture { name: "fox.png".to_owned(), data: vec![1, 2, 3, 254, 255] });
        let chart = ChartSettings { sizing: Sizing::Pixels, tile_mm: 7.5, n_colours: 5, ..ChartSettings::default() };
        let artwork = ArtworkSettings {
            size: Size::Within(120.0, 80.5),
            cell_mm: 0.15,
            mode: ColourMode::Greyscale,
            n_colours: 7,
            background: [10, 20, 30],
            min_patch_mm2: 2.5,
            seed: 9,
        };
        let plates =
            PlateSettings { thickness_mm: 1.6, margin_mm: 6.0, bed_mm: (220.0, 250.0), ..PlateSettings::default() };
        project.set_chart_settings(&chart);
        project.set_artwork_settings(&artwork);
        project.set_plate_settings(&plates);
        project.mode = Mode::Freeform;
        project.fonts = vec![
            Font { name: "NotoSans-Bold.ttf".to_owned(), data: None },
            Font { name: "Mine.otf".to_owned(), data: Some(vec![0, 1, 0, 0]) },
        ];
        project.stencils.merges = vec![[Colour([1, 2, 3]), Colour([4, 5, 6])]];
        project.stencils.skipped = vec![Colour([255, 255, 255])];
        assert_eq!(project.artwork_settings(), artwork);
        assert_eq!(project.plate_settings(), plates);
        // The colours are shared, so tile mode now has the free-form ones.
        assert_eq!(
            project.chart_settings(),
            ChartSettings { mode: ColourMode::Greyscale, n_colours: 7, background: [10, 20, 30], seed: 9, ..chart }
        );
        assert_eq!(project.merges(), [([1, 2, 3], [4, 5, 6])]);

        let file = project.to_json();
        assert_eq!(file, project.to_json(), "the same bytes every time");
        let text = String::from_utf8(file.clone()).unwrap();
        assert!(text.contains(r#""data": "AQID/v8=""#), "{text}");
        assert!(text.contains(r#""within_mm": ["#), "{text}");
        assert!(text.contains(r#""mode": "greyscale""#), "{text}");
        assert_eq!(read_project(&file).unwrap(), project);
    }

    #[test]
    fn mistakes_are_explained() {
        let message = |text: &str| read_project(text.as_bytes()).unwrap_err().to_string();
        assert!(message("not json").starts_with("The project could not be read: expected"));
        assert_eq!(message(r#"{"source": {}}"#), "This is not a Stencilator project.");
        assert_eq!(
            message(r#"{"stencilator": 2}"#),
            "This project was saved by a newer version of Stencilator (format 2); this one opens format 1. \
             Update Stencilator to open it."
        );
        let text = |colour: &str| {
            format!(
                r#"{{"stencilator": 1, "source": {{"kind": "text", "text": "A", "font_mm": 9, "colour": "{colour}"}}}}"#
            )
        };
        assert!(
            message(&text("#12345")).contains(r##""#12345" is not a colour written #rrggbb"##),
            "{}",
            message(&text("#12345"))
        );
        assert!(message(&text("#12345g")).contains("is not a colour"));
        let picture = r#"{"stencilator": 1, "source": {"kind": "picture", "data": "@@@"}}"#;
        assert!(message(picture).contains("an embedded file is damaged"), "{}", message(picture));
        let tiny = r#"{"stencilator": 1, "source": {"kind": "svg", "data": ""}, "tiles": {"tile_mm": 0}}"#;
        assert_eq!(message(tiny), "The tile size must be between 0.1 and 1000 mm.");
    }

    #[test]
    fn the_fox_project_opens_saves_and_makes_plates() {
        use crate::artwork::label_picture;
        use crate::plate::build_plates;

        const FOX: &[u8] = include_bytes!("../../../fixtures/projects/fox.stencilator");
        let project = read_project(FOX).unwrap();
        assert_eq!((project.mode, project.written_by.as_str()), (Mode::Freeform, "0.0.0"));
        // Saved again, the file is the same but for the version that saved it.
        let saved = String::from_utf8(project.to_json()).unwrap();
        let golden = String::from_utf8(FOX.to_vec()).unwrap();
        assert_eq!(saved, golden.replace(r#""written_by": "0.0.0""#, &format!(r#""written_by": "{VERSION}""#)));

        let Source::Picture { name, data } = &project.source else { panic!("the fox is a picture") };
        assert_eq!(name, "fox.png");
        let image = crate::source::decode(data).unwrap();
        let map = label_picture(&image, &project.artwork_settings()).unwrap().merged(&project.merges());
        let plates = build_plates(&map, &project.plate_settings(), &project.skipped()).unwrap();
        let names: Vec<String> = plates.iter().map(crate::plate::Plate::file_name).collect();
        assert_eq!(names, ["plate-01-2b2320.stl", "plate-02-e8772e.stl"]);
    }
}
