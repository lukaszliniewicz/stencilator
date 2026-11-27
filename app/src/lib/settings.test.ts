import { describe, expect, it } from 'vitest';

import {
  equalSettings,
  fromProjectSettings,
  guessSizing,
  isSkipped,
  sanitise,
  toProjectSettings,
  toggledStencil
} from './settings';

describe('settings', () => {
  const defaults = sanitise(undefined);

  it('start from the engine defaults', () => {
    expect(defaults).toEqual({
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
      bridgeWidthMm: null,
      raised: false,
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

  it('keep only well-formed merges and skipped paints', () => {
    const read = sanitise({
      sizing: 'pixels',
      merges: [['#9c3d17', '#e8772e'], ['#9c3d17'], ['#FFFFFF', '#000000'], 'x'],
      skipped: ['#ffffff', 7, '#abc'],
      stencilCanvasColour: 'yes'
    });
    expect(read.sizing).toBe('pixels');
    expect(read.merges).toEqual([['#9c3d17', '#e8772e']]);
    expect(read.skipped).toEqual(['#ffffff']);
    expect(read.stencilCanvasColour).toBe(false);
    expect(sanitise({ sizing: 'stretch', merges: {} }).sizing).toBe('canvas');
  });

  it('leave out-of-range values for the engine to explain', () => {
    expect(sanitise({ canvasWidthMm: -5 }).canvasWidthMm).toBe(-5);
  });

  it('compare by value, with an empty field equal to itself', () => {
    expect(equalSettings(defaults, { ...defaults })).toBe(true);
    expect(equalSettings(defaults, { ...defaults, lineMm: 0.6 })).toBe(false);
    const empty = { ...defaults, tileMm: Number.NaN };
    expect(equalSettings(empty, { ...empty })).toBe(true);
    const merged = { ...defaults, merges: [['#000000', '#ffffff']] as [string, string][] };
    expect(equalSettings(merged, { ...merged, merges: [['#000000', '#ffffff']] })).toBe(true);
    expect(equalSettings(merged, { ...merged, merges: [['#ffffff', '#000000']] })).toBe(false);
    expect(equalSettings(defaults, { ...defaults, skipped: ['#ffffff'] })).toBe(false);
  });

  it('round whole numbers for the engine and keep colours as project hex strings', () => {
    const project = toProjectSettings({
      ...defaults,
      dpi: 299.6,
      colours: 7.4,
      background: '#c02f34',
      merges: [
        ['#9c3d17', '#e8772e'],
        ['#000000', '#ffffff']
      ]
    });
    expect(project.tiles.dpi).toBe(300);
    expect(project.colours.count).toBe(7);
    expect(project.colours.background).toBe('#c02f34');
    expect(project.stencils.merges).toEqual([
      ['#9c3d17', '#e8772e'],
      ['#000000', '#ffffff']
    ]);
  });

  it('round-trips both modes in the project file settings', () => {
    const mixed = {
      ...defaults,
      projectMode: 'freeform',
      sizing: 'pixels',
      sizeMode: 'within',
      artWidthMm: 175,
      artHeightMm: 135,
      bridgeWidthMm: 1.4,
      registrationClearanceMm: 0.35,
      bedDepthMm: 210,
      skipped: ['#202030']
    } as const;
    const p = toProjectSettings({ ...mixed, skipped: [...mixed.skipped] });
    expect(p.freeform.size).toEqual({ within_mm: [175, 135] });
    expect(fromProjectSettings(p)).toEqual(mixed);
  });

  it('take a picture of few pixels for pixel art', () => {
    const picture = (pixelsX: number, pixelsY: number) => ({
      width: pixelsX * 20,
      height: pixelsY * 20,
      pixelsX,
      pixelsY
    });
    expect(guessSizing(picture(22, 22))).toBe('pixels');
    expect(guessSizing(picture(128, 64))).toBe('pixels');
    expect(guessSizing(picture(129, 64))).toBe('canvas');
  });

  it('leave the canvas colour without a stencil until it is ticked', () => {
    const white = '#ffffff';
    const red = '#c02f34';
    expect(isSkipped(defaults, white)).toBe(true);
    expect(isSkipped(defaults, red)).toBe(false);

    const ticked = { ...defaults, ...toggledStencil(defaults, white) };
    expect(isSkipped(ticked, white)).toBe(false);
    const unticked = { ...ticked, ...toggledStencil(ticked, white) };
    expect(isSkipped(unticked, white)).toBe(true);
    expect(unticked.skipped).toEqual([]);

    const noRed = { ...defaults, ...toggledStencil(defaults, red) };
    expect(noRed.skipped).toEqual([red]);
    expect(isSkipped({ ...noRed, ...toggledStencil(noRed, red) }, red)).toBe(false);
    // Another canvas colour, and white is a paint like any other.
    expect(isSkipped({ ...defaults, background: '#000000' }, white)).toBe(false);
  });
});
