/** What the person sets, as the interface keeps it, remembers it and undoes it. */
import type {
  ArtworkSize,
  Mode,
  ProjectMode,
  ProjectSettings,
  Sizing,
  SourceInfo
} from '../engine/client';

export type SizeMode = 'natural' | 'width' | 'height' | 'within';

export interface Settings {
  projectMode: ProjectMode;
  sizing: Sizing;
  canvasWidthMm: number;
  canvasHeightMm: number;
  tileMm: number;
  lineMm: number;
  dpi: number;
  mode: Mode;
  colours: number;
  /** `#rrggbb`, the colour of the canvas, which also shows through transparent parts. */
  background: string;
  /** `#rrggbb`, the colour of the grid lines on the chart. */
  gridColour: string;
  sizeMode: SizeMode;
  artWidthMm: number;
  artHeightMm: number;
  cellMm: number;
  minPatchMm2: number;
  marginMm: number;
  minWebMm: number;
  minOpeningMm: number;
  thicknessMm: number;
  bedWidthMm: number;
  bedDepthMm: number;
  /** Pairs of `#rrggbb`, applied in order: every tile of the first takes the second. */
  merges: [string, string][];
  /** Paints, as `#rrggbb`, that get no stencil. */
  skipped: string[];
  /** Whether a paint of the canvas colour gets a stencil. It rarely needs one. */
  stencilCanvasColour: boolean;
}

/** The engine's own defaults, so the app and the command line agree. */
const DEFAULTS: Readonly<Settings> = {
  projectMode: 'tiles',
  sizing: 'canvas',
  canvasWidthMm: 200,
  canvasHeightMm: 200,
  tileMm: 5,
  lineMm: 0.8,
  dpi: 300,
  mode: 'colour',
  colours: 16,
  background: '#ffffff',
  gridColour: '#000000',
  sizeMode: 'width',
  artWidthMm: 200,
  artHeightMm: 200,
  cellMm: 0.1,
  minPatchMm2: 1,
  marginMm: 10,
  minWebMm: 0.6,
  minOpeningMm: 0.5,
  thicknessMm: 1.2,
  bedWidthMm: 256,
  bedDepthMm: 256,
  merges: [],
  skipped: [],
  stencilCanvasColour: false
};

/** The largest picture, in its own pixels a side, taken for pixel art when it opens. */
const PIXEL_ART_MAX = 128;

const KEYS = Object.keys(DEFAULTS) as (keyof Settings)[];
const MODES: readonly string[] = ['colour', 'greyscale', 'black-white'] satisfies Mode[];
const SIZINGS: readonly string[] = ['pixels', 'canvas'] satisfies Sizing[];
const SIZES: readonly string[] = ['natural', 'width', 'height', 'within'] satisfies SizeMode[];
const HEX = /^#[0-9a-f]{6}$/;

type NumberKey = { [K in keyof Settings]: Settings[K] extends number ? K : never }[keyof Settings];
type ColourKey = 'background' | 'gridColour';

const isHex = (value: unknown): value is string => typeof value === 'string' && HEX.test(value);
const listOf = (value: unknown): unknown[] => (Array.isArray(value) ? (value as unknown[]) : []);
const isPair = (value: unknown): value is [string, string] =>
  Array.isArray(value) && value.length === 2 && isHex(value[0]) && isHex(value[1]);

/**
 * Settings read back from storage. Anything missing or of the wrong kind takes its
 * default; values out of range are left for the engine to explain.
 */
export function sanitise(value: unknown): Settings {
  const stored: Partial<Record<string, unknown>> =
    typeof value === 'object' && value !== null ? value : {};
  const number = (key: NumberKey): number => {
    const v = stored[key];
    return typeof v === 'number' && Number.isFinite(v) ? v : DEFAULTS[key];
  };
  const colour = (key: ColourKey): string => {
    const v = stored[key];
    return isHex(v) ? v : DEFAULTS[key];
  };
  const { mode, sizing, sizeMode } = stored;
  return {
    projectMode: stored.projectMode === 'freeform' ? 'freeform' : DEFAULTS.projectMode,
    sizing:
      typeof sizing === 'string' && SIZINGS.includes(sizing) ? (sizing as Sizing) : DEFAULTS.sizing,
    canvasWidthMm: number('canvasWidthMm'),
    canvasHeightMm: number('canvasHeightMm'),
    tileMm: number('tileMm'),
    lineMm: number('lineMm'),
    dpi: number('dpi'),
    mode: typeof mode === 'string' && MODES.includes(mode) ? (mode as Mode) : DEFAULTS.mode,
    colours: number('colours'),
    background: colour('background'),
    gridColour: colour('gridColour'),
    sizeMode:
      typeof sizeMode === 'string' && SIZES.includes(sizeMode)
        ? (sizeMode as SizeMode)
        : DEFAULTS.sizeMode,
    artWidthMm: number('artWidthMm'),
    artHeightMm: number('artHeightMm'),
    cellMm: number('cellMm'),
    minPatchMm2: number('minPatchMm2'),
    marginMm: number('marginMm'),
    minWebMm: number('minWebMm'),
    minOpeningMm: number('minOpeningMm'),
    thicknessMm: number('thicknessMm'),
    bedWidthMm: number('bedWidthMm'),
    bedDepthMm: number('bedDepthMm'),
    merges: listOf(stored.merges)
      .filter(isPair)
      .map(([from, to]): [string, string] => [from, to]),
    skipped: listOf(stored.skipped).filter(isHex),
    stencilCanvasColour: stored.stencilCanvasColour === true
  };
}

function same(a: unknown, b: unknown): boolean {
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((item, i) => same(item, b[i]));
  }
  return Object.is(a, b);
}

export function equalSettings(a: Settings, b: Settings): boolean {
  return KEYS.every((key) => same(a[key], b[key]));
}

/** An empty input needs a range error from the engine, rather than JSON's null. */
const entered = (value: number): number => (Number.isFinite(value) ? value : -1);
const whole = (value: number): number => {
  const n = Math.round(value);
  return n >= 0 && n <= 0xffffffff ? n : 0;
};

function artworkSize(settings: Settings): ArtworkSize {
  const w = entered(settings.artWidthMm);
  const h = entered(settings.artHeightMm);
  switch (settings.sizeMode) {
    case 'natural':
      return 'natural';
    case 'width':
      return { width_mm: w };
    case 'height':
      return { height_mm: h };
    case 'within':
      return { within_mm: [w, h] };
  }
}

export function toProjectSettings(s: Settings): ProjectSettings {
  return {
    mode: s.projectMode,
    colours: { mode: s.mode, count: whole(s.colours), background: s.background },
    tiles: {
      sizing: s.sizing,
      canvas_mm: [entered(s.canvasWidthMm), entered(s.canvasHeightMm)],
      tile_mm: entered(s.tileMm),
      line_mm: entered(s.lineMm),
      dpi: whole(s.dpi),
      grid_colour: s.gridColour
    },
    freeform: {
      size: artworkSize(s),
      cell_mm: entered(s.cellMm),
      min_patch_mm2: entered(s.minPatchMm2),
      margin_mm: entered(s.marginMm),
      min_web_mm: entered(s.minWebMm),
      min_opening_mm: entered(s.minOpeningMm)
    },
    stencils: {
      thickness_mm: entered(s.thicknessMm),
      bed_mm: [entered(s.bedWidthMm), entered(s.bedDepthMm)],
      merges: s.merges,
      skipped: s.skipped,
      stencil_canvas_colour: s.stencilCanvasColour
    }
  };
}

/** Settings from a project file have already been normalised and checked by the engine. */
export function fromProjectSettings(p: ProjectSettings): Settings {
  const t = p.tiles;
  const f = p.freeform;
  const s = p.stencils;
  const settings: Settings = {
    ...DEFAULTS,
    projectMode: p.mode,
    sizing: t.sizing,
    canvasWidthMm: t.canvas_mm[0],
    canvasHeightMm: t.canvas_mm[1],
    tileMm: t.tile_mm,
    lineMm: t.line_mm,
    dpi: t.dpi,
    gridColour: t.grid_colour,
    mode: p.colours.mode,
    colours: p.colours.count,
    background: p.colours.background,
    cellMm: f.cell_mm,
    minPatchMm2: f.min_patch_mm2,
    marginMm: f.margin_mm,
    minWebMm: f.min_web_mm,
    minOpeningMm: f.min_opening_mm,
    thicknessMm: s.thickness_mm,
    bedWidthMm: s.bed_mm[0],
    bedDepthMm: s.bed_mm[1],
    merges: s.merges,
    skipped: s.skipped,
    stencilCanvasColour: s.stencil_canvas_colour
  };
  if (f.size === 'natural') settings.sizeMode = 'natural';
  else if ('width_mm' in f.size) settings.artWidthMm = f.size.width_mm;
  else if ('height_mm' in f.size) {
    settings.sizeMode = 'height';
    settings.artHeightMm = f.size.height_mm;
  } else {
    settings.sizeMode = 'within';
    [settings.artWidthMm, settings.artHeightMm] = f.size.within_mm;
  }
  return settings;
}

/** Pixel art gets a tile per pixel; anything larger is fitted to the canvas. */
export function guessSizing(source: SourceInfo): Sizing {
  return Math.max(source.pixelsX, source.pixelsY) <= PIXEL_ART_MAX ? 'pixels' : 'canvas';
}

/** Whether a paint goes without a stencil. */
export function isSkipped(settings: Settings, colour: string): boolean {
  return (
    settings.skipped.includes(colour) ||
    (colour === settings.background && !settings.stencilCanvasColour)
  );
}

/** The settings that change when a paint's stencil is turned on or off. */
export function toggledStencil(
  settings: Settings,
  colour: string
): Pick<Settings, 'skipped' | 'stencilCanvasColour'> {
  const skipped = settings.skipped.filter((c) => c !== colour);
  const canvas = colour === settings.background;
  if (isSkipped(settings, colour)) {
    return { skipped, stencilCanvasColour: settings.stencilCanvasColour || canvas };
  }
  return canvas
    ? { skipped, stencilCanvasColour: false }
    : { skipped: [...skipped, colour], stencilCanvasColour: settings.stencilCanvasColour };
}
