/** What the person sets, as the interface keeps it, remembers it and undoes it. */
import type {
  ArtworkSize,
  Mark,
  Mode,
  ProjectMode,
  ProjectSettings,
  Sizing,
  SourceInfo,
  SupportEdit,
  SupportKind
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
  supportKind: SupportKind | 'none';
  bridgeWidthMm: number;
  connectorWidthMm: number;
  connectorClearanceMm: number;
  beamMm: number;
  warnSpanMm: number;
  supportEdits: SupportEdit[];
  registrationClearanceMm: number | null;
  paintOrder: string[];
  mark: Mark;
  chamferMm: number;
  overlapMm: number;
  matchRal: boolean;
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
  supportKind: 'none',
  bridgeWidthMm: 1.2,
  connectorWidthMm: 1.2,
  connectorClearanceMm: 4,
  beamMm: 1,
  warnSpanMm: 40,
  supportEdits: [],
  registrationClearanceMm: null,
  paintOrder: [],
  mark: 'none',
  chamferMm: 0,
  overlapMm: 0,
  matchRal: false,
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
  const { mode, sizing, sizeMode, supportKind } = stored;
  // Older local settings represented the choice as a nullable bridge width.
  const savedKind =
    stored.raised === true ? 'raised' : typeof stored.bridgeWidthMm === 'number' ? 'flat' : 'none';
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
    supportKind:
      supportKind === 'none' || supportKind === 'flat' || supportKind === 'raised'
        ? supportKind
        : savedKind,
    bridgeWidthMm: number('bridgeWidthMm'),
    connectorWidthMm: number('connectorWidthMm'),
    connectorClearanceMm: number('connectorClearanceMm'),
    beamMm: number('beamMm'),
    warnSpanMm: number('warnSpanMm'),
    supportEdits: [],
    registrationClearanceMm: null,
    paintOrder: [],
    mark: stored.mark === 'number' || stored.mark === 'ral' ? stored.mark : 'none',
    chamferMm: number('chamferMm'),
    overlapMm: number('overlapMm'),
    matchRal: stored.matchRal === true,
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

type NumberRange = readonly [number, number];

/**
 * The engine's accepted settings ranges, which a saved default must meet in every mode.
 * The interface leaves a live out-of-range entry for the engine to explain; a restored or
 * migrated default offered for new projects has no such moment. As storage's `sanitise`
 * does for a missing or malformed value, an unusable range takes the engine's default
 * rather than rejecting the saved preferences, and every valid explicit value is kept.
 *
 * A default seeds projects in any mode and presets may switch sizing or mode, so every
 * field is checked even while its mode is inactive. This is deliberately stricter than a
 * single engine check, which ignores inactive fields: a default is only useful if it stays
 * usable after such a switch. Paint merges and skipped paints are valid lists, not ranges,
 * and are kept exactly as saved, without deduplication or truncation.
 */
const DEFAULT_RANGES: Partial<Record<NumberKey, NumberRange>> = {
  canvasWidthMm: [1, 10_000],
  canvasHeightMm: [1, 10_000],
  tileMm: [0.1, 1_000],
  lineMm: [0, 20],
  artWidthMm: [1, 10_000],
  artHeightMm: [1, 10_000],
  cellMm: [0.05, 0.5],
  minPatchMm2: [0, 100],
  marginMm: [2, 100],
  minWebMm: [0.1, 5],
  minOpeningMm: [0.1, 5],
  bridgeWidthMm: [0.6, 5],
  connectorWidthMm: [0.6, 5],
  connectorClearanceMm: [0.4, 30],
  beamMm: [0.4, 5],
  warnSpanMm: [1, 200],
  thicknessMm: [0.2, 20],
  bedWidthMm: [50, 2_000],
  bedDepthMm: [50, 2_000]
};

/** Ranges checked after the engine's rounding of a whole number. */
const DEFAULT_WHOLE_RANGES: Partial<Record<NumberKey, NumberRange>> = {
  colours: [1, 256],
  dpi: [72, 2_400]
};

/** Settings that accept zero, such as no chamfer, or a value within a range. */
const DEFAULT_ZERO_OR_RANGES: Partial<Record<NumberKey, NumberRange>> = {
  chamferMm: [0.1, 2],
  overlapMm: [0.1, 2]
};

const inRange = (value: number, [min, max]: NumberRange): boolean =>
  Number.isFinite(value) && value >= min && value <= max;

/** Whole micrometres, rounding as the engine does, for its thickness comparisons. */
function micrometres(mm: number): number {
  const scaled = mm * 1_000;
  const lower = Math.floor(scaled);
  const fraction = scaled - lower;
  if (fraction < 0.5) return lower;
  if (fraction > 0.5) return lower + 1;
  return lower % 2 === 0 ? lower : lower + 1;
}

/**
 * Saved defaults with unusable values replaced, so new projects can always be saved.
 * This seeds a project or a set of migrated preferences; live artwork and project edits
 * still go through `sanitise`, which leaves an out-of-range entry for the engine to explain.
 */
export function normaliseDefaults(raw: unknown): Settings {
  const settings = sanitise(raw);
  const fallback = sanitise(undefined);
  for (const [key, range] of Object.entries(DEFAULT_RANGES) as [NumberKey, NumberRange][]) {
    if (!inRange(settings[key], range)) settings[key] = fallback[key];
  }
  for (const [key, range] of Object.entries(DEFAULT_WHOLE_RANGES) as [NumberKey, NumberRange][]) {
    if (!inRange(Math.round(settings[key]), range)) settings[key] = fallback[key];
  }
  for (const [key, range] of Object.entries(DEFAULT_ZERO_OR_RANGES) as [NumberKey, NumberRange][]) {
    if (settings[key] !== 0 && !inRange(settings[key], range)) settings[key] = fallback[key];
  }
  // A raised connector needs 0.2 mm of clearance above the plate itself. Compare whole
  // micrometres, as the engine does, so a value it accepts is never changed by rounding.
  const thickness = micrometres(settings.thicknessMm);
  if (micrometres(settings.connectorClearanceMm) - thickness < 200) {
    settings.connectorClearanceMm = Math.max(
      fallback.connectorClearanceMm,
      (thickness + 200) / 1_000
    );
  }
  // A chamfer must leave 0.2 mm of plate at the work face.
  if (settings.chamferMm !== 0 && thickness - micrometres(settings.chamferMm) < 200) {
    settings.chamferMm = fallback.chamferMm;
  }
  return settings;
}

function same(a: unknown, b: unknown): boolean {
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((item, i) => same(item, b[i]));
  }
  return Object.is(a, b);
}

export function equalSettings(a: Settings, b: Settings): boolean {
  return KEYS.every((key) =>
    key === 'supportEdits'
      ? a.supportEdits.length === b.supportEdits.length &&
        a.supportEdits.every((edit, i) => {
          const other = b.supportEdits[i];
          return (
            edit.paint === other?.paint &&
            edit.kind === other.kind &&
            same(edit.original, other.original) &&
            same(edit.ends, other.ends)
          );
        })
      : same(a[key], b[key])
  );
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
  const raised = {
    width_mm: entered(s.connectorWidthMm),
    clearance_mm: entered(s.connectorClearanceMm),
    beam_mm: entered(s.beamMm),
    warn_mm: entered(s.warnSpanMm)
  };
  const customFlat = Number.isFinite(s.bridgeWidthMm) && s.bridgeWidthMm !== DEFAULTS.bridgeWidthMm;
  const customRaised =
    [s.connectorWidthMm, s.connectorClearanceMm, s.beamMm, s.warnSpanMm].every(Number.isFinite) &&
    (s.connectorWidthMm !== DEFAULTS.connectorWidthMm ||
      s.connectorClearanceMm !== DEFAULTS.connectorClearanceMm ||
      s.beamMm !== DEFAULTS.beamMm ||
      s.warnSpanMm !== DEFAULTS.warnSpanMm);
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
      min_opening_mm: entered(s.minOpeningMm),
      ...(s.supportKind === 'flat' ? { bridge_width_mm: entered(s.bridgeWidthMm) } : {}),
      ...(s.supportKind === 'raised' ? { raised_connectors: raised } : {}),
      ...(s.supportKind !== 'flat' && customFlat
        ? { inactive_bridge_width_mm: s.bridgeWidthMm }
        : {}),
      ...(s.supportKind !== 'raised' && customRaised ? { inactive_raised_connectors: raised } : {}),
      ...(s.supportEdits.length ? { support_edits: s.supportEdits } : {}),
      ...(s.registrationClearanceMm !== null
        ? { registration_clearance_mm: entered(s.registrationClearanceMm) }
        : {}),
      ...(s.paintOrder.length ? { paint_order: s.paintOrder } : {}),
      ...(s.chamferMm !== 0 || s.overlapMm !== 0 || s.mark !== 'none' || s.matchRal
        ? {
            finish: {
              chamfer_mm: entered(s.chamferMm),
              overlap_mm: entered(s.overlapMm),
              mark: s.mark,
              match_ral: s.matchRal
            }
          }
        : {})
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
  const raised = f.raised_connectors ?? f.inactive_raised_connectors;
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
    supportKind: f.raised_connectors ? 'raised' : f.bridge_width_mm !== undefined ? 'flat' : 'none',
    bridgeWidthMm: f.bridge_width_mm ?? f.inactive_bridge_width_mm ?? DEFAULTS.bridgeWidthMm,
    connectorWidthMm: raised?.width_mm ?? DEFAULTS.connectorWidthMm,
    connectorClearanceMm: raised?.clearance_mm ?? DEFAULTS.connectorClearanceMm,
    beamMm: raised?.beam_mm ?? DEFAULTS.beamMm,
    warnSpanMm: raised?.warn_mm ?? DEFAULTS.warnSpanMm,
    supportEdits: f.support_edits ?? [],
    registrationClearanceMm: f.registration_clearance_mm ?? null,
    paintOrder: f.paint_order ?? [],
    mark: f.finish?.mark ?? DEFAULTS.mark,
    chamferMm: f.finish?.chamfer_mm ?? DEFAULTS.chamferMm,
    overlapMm: f.finish?.overlap_mm ?? DEFAULTS.overlapMm,
    matchRal: f.finish?.match_ral ?? DEFAULTS.matchRal,
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
