/** Messages between the interface and the engine's Web Worker. */

export type Mode = 'colour' | 'greyscale' | 'black-white';
export type ChartFormat = 'png' | 'jpeg' | 'pdf';

/** Everything that decides the grid and the palette. Colours are `0xRRGGBB`. */
export interface ChartSettings {
  canvasWidthMm: number;
  canvasHeightMm: number;
  tileMm: number;
  lineMm: number;
  dpi: number;
  mode: Mode;
  colours: number;
  background: number;
}

/** The engine's version, and the thinnest web between holes it recommends. */
export interface About {
  version: string;
  minWebMm: number;
}

export interface SourceInfo {
  width: number;
  height: number;
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

export type Request =
  | { kind: 'about' }
  | { kind: 'open'; bytes: ArrayBuffer }
  | { kind: 'chart'; settings: ChartSettings }
  | { kind: 'chartFile'; format: ChartFormat; gridColour: number; withLines: boolean }
  | { kind: 'stencil'; index: number; thicknessMm: number }
  | { kind: 'stencilsZip'; thicknessMm: number };

/** What each kind of request answers with. */
export interface Replies {
  about: About;
  open: SourceInfo;
  chart: ChartSummary;
  chartFile: Uint8Array;
  stencil: Uint8Array;
  stencilsZip: Uint8Array;
}

export type Kind = Request['kind'];
export type RequestOf<K extends Kind> = Extract<Request, { kind: K }>;

export interface Envelope {
  id: number;
  request: Request;
}

export type Reply =
  { id: number; ok: true; value: Replies[Kind] } | { id: number; ok: false; message: string };
