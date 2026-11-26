import { describe, expect, it } from 'vitest';

import { equalSettings, sanitise, toChartSettings } from './settings';

describe('settings', () => {
  const defaults = sanitise(undefined);

  it('start from the engine defaults', () => {
    expect(defaults).toEqual({
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
    });
  });

  it('keep what was stored and replace what is broken', () => {
    const read = sanitise({
      canvasWidthMm: 400,
      canvasHeightMm: '300',
      tileMm: null,
      dpi: Number.POSITIVE_INFINITY,
      mode: 'greyscale',
      background: '#123abc',
      gridColour: 'red',
      extra: true
    });
    expect(read.canvasWidthMm).toBe(400);
    expect(read.canvasHeightMm).toBe(200);
    expect(read.tileMm).toBe(5);
    expect(read.dpi).toBe(300);
    expect(read.mode).toBe('greyscale');
    expect(read.background).toBe('#123abc');
    expect(read.gridColour).toBe('#000000');
    expect(read).not.toHaveProperty('extra');
    expect(sanitise({ mode: 'sepia' }).mode).toBe('colour');
    expect(sanitise('nonsense')).toEqual(defaults);
  });

  it('leave out-of-range values for the engine to explain', () => {
    expect(sanitise({ canvasWidthMm: -5 }).canvasWidthMm).toBe(-5);
  });

  it('compare by value, with an empty field equal to itself', () => {
    expect(equalSettings(defaults, { ...defaults })).toBe(true);
    expect(equalSettings(defaults, { ...defaults, lineMm: 0.6 })).toBe(false);
    const empty = { ...defaults, tileMm: Number.NaN };
    expect(equalSettings(empty, { ...empty })).toBe(true);
  });

  it('round whole numbers for the engine and pass colours as numbers', () => {
    const chart = toChartSettings({
      ...defaults,
      dpi: 299.6,
      colours: 7.4,
      background: '#c02f34'
    });
    expect(chart.dpi).toBe(300);
    expect(chart.colours).toBe(7);
    expect(chart.background).toBe(0xc02f34);
    expect(chart).not.toHaveProperty('thicknessMm');
  });
});
