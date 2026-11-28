/** Messages between the interface and the engine's Web Worker. */

export type Mode = 'colour' | 'greyscale' | 'black-white';
/** One tile for each of the picture's own pixels, or the picture fitted to a canvas. */
export type Sizing = 'pixels' | 'canvas';
export type ChartFormat = 'png' | 'jpeg' | 'pdf';
export type ProjectMode = 'tiles' | 'freeform';
export type BedFit = 'as-drawn' | 'quarter-turn' | 'too-large';
export type SupportKind = 'flat' | 'raised';
export type Mark = 'none' | 'number' | 'ral';
export type SupportIssue = 'landings' | 'span' | 'conflict' | 'suggestion';
export type Point = [number, number];
/** Whole micrometres from the plate's bottom-left. */
export type Ends = [Point, Point];

export interface SupportEdit {
  paint: string;
  kind: SupportKind;
  original?: Ends;
  ends?: Ends;
}

interface RaisedSettings {
  width_mm: number;
  clearance_mm: number;
  beam_mm: number;
  warn_mm: number;
}

export type ArtworkSize =
  'natural' | { width_mm: number } | { height_mm: number } | { within_mm: [number, number] };

/** The editable settings, in the project file's form. The engine keeps the seed. */
export interface ProjectSettings {
  mode: ProjectMode;
  colours: { mode: Mode; count: number; background: string };
  tiles: {
    sizing: Sizing;
    canvas_mm: [number, number];
    tile_mm: number;
    line_mm: number;
    dpi: number;
    grid_colour: string;
  };
  freeform: {
    size: ArtworkSize;
    cell_mm: number;
    min_patch_mm2: number;
    margin_mm: number;
    min_web_mm: number;
    min_opening_mm: number;
    bridge_width_mm?: number;
    raised_connectors?: RaisedSettings;
    inactive_bridge_width_mm?: number;
    inactive_raised_connectors?: RaisedSettings;
    registration_clearance_mm?: number;
    support_edits?: SupportEdit[];
    paint_order?: string[];
    finish?: { chamfer_mm: number; overlap_mm: number; mark: Mark; match_ral: boolean };
  };
  stencils: {
    thickness_mm: number;
    bed_mm: [number, number];
    merges: [string, string][];
    skipped: string[];
    stencil_canvas_colour: boolean;
  };
}

/** The engine's version, and the thinnest web between holes it recommends. */
export interface About {
  version: string;
  minWebMm: number;
}

export interface SourceInfo {
  width: number;
  height: number;
  /** The picture's own pixels, where a block of one colour drawn larger counts as one. */
  pixelsX: number;
  pixelsY: number;
}

export interface TypedText {
  value: string;
  fontMm: number;
  colour: string;
}

export type ArtworkSource =
  | { kind: 'picture'; name: string; info: SourceInfo }
  | { kind: 'svg'; name: string }
  | { kind: 'text'; style: TypedText };

export interface ProjectFont {
  name: string;
  /** A built-in font needs only its name. */
  data?: Blob;
}

export interface FontSelection {
  revision: number;
  files: readonly ProjectFont[];
}

export interface OpenedProject {
  source: ArtworkSource;
  settings: ProjectSettings;
  fonts: ProjectFont[];
  /** Only sent when a project file supplied the picture. */
  pictureBytes?: Uint8Array;
}

export interface ArtworkEdits {
  typed?: TypedText;
  fonts: FontSelection;
}

/** A built chart, as the interface draws and describes it. */
export interface ChartSummary {
  tilesX: number;
  tilesY: number;
  tilePx: number;
  linePx: number;
  tileMm: number;
  lineMm: number;
  widthMm: number;
  heightMm: number;
  bedFit: BedFit | undefined;
  widthPx: number;
  heightPx: number;
  /** RGB triples, darkest first. */
  palette: Uint8Array;
  /** The palette index of every tile, row by row from the top. */
  labels: Uint32Array;
  /** Tiles of each palette colour. */
  counts: Uint32Array;
  /** The file name of each colour's stencil. */
  stencilNames: string[];
}

export interface RasterPreview {
  width: number;
  height: number;
  rgba: Uint8Array;
}

interface Spot {
  x_mm: number;
  y_mm: number;
  width_mm: number;
  height_mm: number;
  area_mm2: number;
}

export interface PlateReport {
  paint: number;
  colour: string;
  width_mm: number;
  height_mm: number;
  openings: number;
  paint_mm2: number;
  opening_mm2: number;
  islands: Spot[];
  thin_webs: Spot[];
  narrow_openings: Spot[];
  fits_bed: boolean;
  bed_fit: BedFit;
  flat_bridges?: { count: number; covered_mm2: number; single_bridge_islands: Spot[] };
  raised_connectors?: { count: number; single_support_islands: Spot[]; long_spans: number };
  height_with_supports_mm?: number;
  unplaced_supports?: number;
  unmatched_suggestions?: number;
  mark?: { text: string; applied: boolean };
  ral?: { code: number; colour: string };
  chamfer_limited?: boolean;
  overlap_limited?: boolean;
  stencil: string;
}

export interface PlateDrawing {
  width_mm: number;
  height_mm: number;
  plate: string;
  islands: string;
  thin_webs: string;
  narrow_openings: string;
  chamfer?: string;
  mark?: string;
  supports?: SupportDrawing[];
}

export interface SupportDrawing {
  ends: Ends;
  path: string;
  span_mm: number;
  edit?: number;
  issue?: SupportIssue;
}

export interface Coat {
  paint: number;
  colour: string;
  path: string;
}

export interface FrameReport {
  width_mm: number;
  height_mm: number;
  clearance_mm: number;
  fits_bed: boolean;
  bed_fit: BedFit;
  file: string;
}

export interface FreeformSummary {
  widthMm: number;
  heightMm: number;
  cellMm: number;
  thicknessMm: number;
  background: string;
  palette: Uint8Array;
  counts: Uint32Array;
  artwork?: RasterPreview;
  painted: RasterPreview;
  plates: PlateReport[];
  frame?: FrameReport;
}

export type FileKind = 'picture' | 'svg' | 'project';
export type FreeformStage = 'paints' | 'plates';

export type Request =
  | { kind: 'about' }
  | { kind: 'open'; file: FileKind; name: string; bytes: ArrayBuffer }
  | { kind: 'typeText'; value: string }
  | { kind: 'chart'; settings: ProjectSettings }
  | { kind: 'chartFile'; format: ChartFormat; gridColour: number; withLines: boolean }
  | { kind: 'stencil'; index: number; thicknessMm: number }
  | { kind: 'stencilsZip'; thicknessMm: number; skip: number[] }
  | { kind: 'freeform'; settings: ProjectSettings; edits: ArtworkEdits }
  | { kind: 'plateDrawing'; index: number }
  | { kind: 'supportIssue'; index: number; ends: Ends; edit: number | undefined }
  | { kind: 'paintPaths' }
  | { kind: 'plateFile'; index: number }
  | { kind: 'frameFile' }
  | { kind: 'platesZip' }
  | { kind: 'threeMf' }
  | { kind: 'projectFile'; settings: ProjectSettings; edits: ArtworkEdits; strict?: boolean }
  | { kind: 'validateProject'; bytes: ArrayBuffer };

/** What each kind of request answers with. */
export interface Replies {
  about: About;
  open: OpenedProject;
  typeText: OpenedProject;
  chart: ChartSummary;
  chartFile: Uint8Array;
  stencil: Uint8Array;
  stencilsZip: Uint8Array;
  freeform: FreeformSummary;
  plateDrawing: PlateDrawing;
  supportIssue: SupportIssue | undefined;
  paintPaths: Coat[];
  plateFile: Uint8Array;
  frameFile: Uint8Array;
  platesZip: Uint8Array;
  threeMf: Uint8Array;
  projectFile: Uint8Array;
  validateProject: ProjectMode | null;
}

export type Kind = Request['kind'];
export type RequestOf<K extends Kind> = Extract<Request, { kind: K }>;

export interface Envelope {
  id: number;
  request: Request;
}

export type ProjectSaveProblem =
  | { kind: 'settings' }
  | { kind: 'inactive-settings'; mode: ProjectMode }
  | { kind: 'artwork' }
  | { kind: 'file' };

export class ProjectSaveError extends Error {
  constructor(
    readonly problem: ProjectSaveProblem,
    options?: ErrorOptions
  ) {
    super(problem.kind, options);
    this.name = 'ProjectSaveError';
  }
}

export type Reply =
  | { id: number; ok: true; value: Replies[Kind] }
  | { id: number; ok: false; saveProblem: ProjectSaveProblem }
  | { id: number; ok: false; message: string }
  | { id: number; stage: FreeformStage };
