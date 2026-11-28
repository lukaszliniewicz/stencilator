import { describe, expect, it } from 'vitest';

import {
  equalSettings,
  fromProjectSettings,
  guessSizing,
  isSkipped,
  normaliseDefaults,
  sanitise,
  toProjectSettings,
  toggledStencil,
  type Settings
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

  it('repair saved defaults before they seed a project', () => {
    const repaired = normaliseDefaults({
      cellMm: 3,
      bedWidthMm: 40,
      marginMm: 1,
      thicknessMm: 50,
      connectorClearanceMm: 1,
      chamferMm: 3
    });
    expect(repaired).toMatchObject({
      cellMm: 0.1,
      bedWidthMm: 256,
      marginMm: 10,
      thicknessMm: 1.2,
      connectorClearanceMm: 4,
      chamferMm: 0
    });
    expect(normaliseDefaults(undefined)).toEqual(defaults);
  });

  it('keep valid explicit saved defaults and replace only what is unusable', () => {
    const kept = normaliseDefaults({
      cellMm: 0.2,
      bedWidthMm: 300,
      bedDepthMm: 320,
      tileMm: 12,
      colours: 32,
      marginMm: 25,
      supportKind: 'flat',
      bridgeWidthMm: 1.8,
      skipped: ['#202030']
    });
    expect(kept).toMatchObject({
      cellMm: 0.2,
      bedWidthMm: 300,
      bedDepthMm: 320,
      tileMm: 12,
      colours: 32,
      marginMm: 25,
      supportKind: 'flat',
      bridgeWidthMm: 1.8,
      skipped: ['#202030']
    });
    // Fields left out still take the engine's default.
    expect(kept.canvasWidthMm).toBe(200);
    expect(kept.artWidthMm).toBe(200);
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
      supportKind: 'flat',
      bridgeWidthMm: 1.4,
      registrationClearanceMm: 0.35,
      bedDepthMm: 210,
      skipped: ['#202030']
    } as const;
    const p = toProjectSettings({ ...mixed, skipped: [...mixed.skipped] });
    expect(p.freeform.size).toEqual({ within_mm: [175, 135] });
    expect(fromProjectSettings(p)).toEqual(mixed);
  });

  it('keeps both support sizes and the finishing when the active support changes', () => {
    const settings: Settings = {
      ...defaults,
      supportKind: 'raised',
      bridgeWidthMm: 1.6,
      connectorWidthMm: 2.3,
      connectorClearanceMm: 5.2,
      beamMm: 0.9,
      warnSpanMm: 33,
      paintOrder: ['#193559', '#ffffff'],
      mark: 'ral',
      chamferMm: 0.35,
      overlapMm: 0.2,
      matchRal: true
    };
    for (const supportKind of ['none', 'flat', 'raised'] as const) {
      const selected = { ...settings, supportKind };
      const project = toProjectSettings(selected);
      expect(project.freeform.raised_connectors !== undefined).toBe(supportKind === 'raised');
      expect(project.freeform.bridge_width_mm !== undefined).toBe(supportKind === 'flat');
      expect(fromProjectSettings(project)).toEqual(selected);
    }
    expect(sanitise({ bridgeWidthMm: 1.8 })).toMatchObject({
      supportKind: 'flat',
      bridgeWidthMm: 1.8
    });
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
