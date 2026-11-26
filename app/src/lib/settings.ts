/** What the person sets, as the interface keeps it, remembers it and undoes it. */
import type { ChartSettings, Mode, Sizing, SourceInfo } from '../engine/client';
import { hexToNumber } from './colour';

export interface Settings {
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
  thicknessMm: number;
  /** Pairs of `#rrggbb`, applied in order: every tile of the first takes the second. */
  merges: [string, string][];
  /** Paints, as `#rrggbb`, that get no stencil. */
  skipped: string[];
  /** Whether a paint of the canvas colour gets a stencil. It rarely needs one. */
  stencilCanvasColour: boolean;
}

/** The engine's own defaults, so the app and the command line agree. */
const DEFAULTS: Readonly<Settings> = {
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
  thicknessMm: 1.2,
  merges: [],
  skipped: [],
  stencilCanvasColour: false
};

/** The largest picture, in its own pixels a side, taken for pixel art when it opens. */
const PIXEL_ART_MAX = 128;

const KEYS = Object.keys(DEFAULTS) as (keyof Settings)[];
const MODES: readonly string[] = ['colour', 'greyscale', 'black-white'] satisfies Mode[];
const SIZINGS: readonly string[] = ['pixels', 'canvas'] satisfies Sizing[];
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
  const { mode, sizing } = stored;
  return {
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
    thicknessMm: number('thicknessMm'),
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

/** What the engine builds a chart from. Whole numbers are rounded, not truncated. */
export function toChartSettings(settings: Settings): ChartSettings {
  return {
    sizing: settings.sizing,
    canvasWidthMm: settings.canvasWidthMm,
    canvasHeightMm: settings.canvasHeightMm,
    tileMm: settings.tileMm,
    lineMm: settings.lineMm,
    dpi: Math.round(settings.dpi),
    mode: settings.mode,
    colours: Math.round(settings.colours),
    background: hexToNumber(settings.background),
    merges: settings.merges.flatMap(([from, to]) => [hexToNumber(from), hexToNumber(to)])
  };
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
