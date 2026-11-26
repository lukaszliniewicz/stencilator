/** What the person sets, as the interface keeps it, remembers it and undoes it. */
import type { ChartSettings, Mode } from '../engine/client';
import { hexToNumber } from './colour';

export interface Settings {
  canvasWidthMm: number;
  canvasHeightMm: number;
  tileMm: number;
  lineMm: number;
  dpi: number;
  mode: Mode;
  colours: number;
  /** `#rrggbb`, laid behind transparent parts of the picture. */
  background: string;
  /** `#rrggbb`, the colour of the grid lines on the chart. */
  gridColour: string;
  thicknessMm: number;
}

/** The engine's own defaults, so the app and the command line agree. */
const DEFAULTS: Readonly<Settings> = {
  canvasWidthMm: 200,
  canvasHeightMm: 200,
  tileMm: 5,
  lineMm: 0.8,
  dpi: 300,
  mode: 'colour',
  colours: 16,
  background: '#ffffff',
  gridColour: '#000000',
  thicknessMm: 1.2
};

const KEYS = Object.keys(DEFAULTS) as (keyof Settings)[];
const MODES: readonly string[] = ['colour', 'greyscale', 'black-white'] satisfies Mode[];
const HEX = /^#[0-9a-f]{6}$/;

type NumberKey = { [K in keyof Settings]: Settings[K] extends number ? K : never }[keyof Settings];
type ColourKey = 'background' | 'gridColour';

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
    return typeof v === 'string' && HEX.test(v) ? v : DEFAULTS[key];
  };
  const mode = stored.mode;
  return {
    canvasWidthMm: number('canvasWidthMm'),
    canvasHeightMm: number('canvasHeightMm'),
    tileMm: number('tileMm'),
    lineMm: number('lineMm'),
    dpi: number('dpi'),
    mode: typeof mode === 'string' && MODES.includes(mode) ? (mode as Mode) : DEFAULTS.mode,
    colours: number('colours'),
    background: colour('background'),
    gridColour: colour('gridColour'),
    thicknessMm: number('thicknessMm')
  };
}

export function equalSettings(a: Settings, b: Settings): boolean {
  return KEYS.every((key) => Object.is(a[key], b[key]));
}

/** What the engine builds a chart from. Whole numbers are rounded, not truncated. */
export function toChartSettings(settings: Settings): ChartSettings {
  return {
    canvasWidthMm: settings.canvasWidthMm,
    canvasHeightMm: settings.canvasHeightMm,
    tileMm: settings.tileMm,
    lineMm: settings.lineMm,
    dpi: Math.round(settings.dpi),
    mode: settings.mode,
    colours: Math.round(settings.colours),
    background: hexToNumber(settings.background)
  };
}
