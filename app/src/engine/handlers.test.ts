/// <reference types="node" />
import { readFileSync } from 'node:fs';

import { beforeAll, describe, expect, it } from 'vitest';

import { createHandlers } from './handlers';
import type { ChartSettings, Kind, Replies, RequestOf } from './protocol';
import * as engine from './wasm/stencilator_wasm.js';

const fixture = (name: string) => new URL(`../../../fixtures/${name}`, import.meta.url);

interface ReferenceChart {
  image: string;
  settings: { canvas_mm: [number, number]; tile_mm: number; line_mm: number; dpi: number };
  tiles: [number, number];
  palette: [number, number, number][];
  labels: number[][];
}

const reference = (
  JSON.parse(readFileSync(fixture('reference/charts.json'), 'utf8')) as { cases: ReferenceChart[] }
).cases[0];

const SETTINGS: ChartSettings = {
  sizing: 'canvas',
  canvasWidthMm: 47,
  canvasHeightMm: 40,
  tileMm: 5,
  lineMm: 0.8,
  dpi: 300,
  mode: 'colour',
  colours: 16,
  background: 0xffffff,
  merges: []
};

/** Files in a zip, counted by their entries in its central directory. */
const entries = (zip: Uint8Array): number =>
  zip.reduce(
    (n, byte, i) =>
      byte === 0x50 && zip[i + 1] === 0x4b && zip[i + 2] === 0x01 && zip[i + 3] === 0x02
        ? n + 1
        : n,
    0
  );

const bytesOf = (name: string): ArrayBuffer => {
  const file = readFileSync(fixture(name));
  return file.buffer.slice(file.byteOffset, file.byteOffset + file.byteLength);
};

describe('the engine in WebAssembly', () => {
  let handle: ReturnType<typeof createHandlers>;
  const ask = <K extends Kind>(request: RequestOf<K>): Replies[K] =>
    handle(request).value as Replies[K];

  beforeAll(() => {
    engine.initSync({
      module: readFileSync(new URL('wasm/stencilator_wasm_bg.wasm', import.meta.url))
    });
    handle = createHandlers(engine);
  });

  it('tells its version and the thinnest web it recommends', () => {
    const about = ask({ kind: 'about' });
    expect(about.version).toMatch(/^\d+\.\d+\.\d+/);
    expect(about.minWebMm).toBeGreaterThan(0);
  });

  it('refuses files before there is a chart', () => {
    expect(() => ask({ kind: 'chart', settings: SETTINGS })).toThrow('Open an image first.');
    expect(() => ask({ kind: 'stencilsZip', thicknessMm: 1.2, skip: [] })).toThrow(
      'There is no chart'
    );
  });

  it('builds the reference chart', () => {
    if (!reference) throw new Error('no reference chart');
    const info = ask({ kind: 'open', bytes: bytesOf(`reference/${reference.image}`) });
    expect(info.width).toBeGreaterThan(0);
    const chart = ask({ kind: 'chart', settings: SETTINGS });
    expect([chart.tilesX, chart.tilesY]).toEqual(reference.tiles);
    expect([...chart.palette]).toEqual(reference.palette.flat());
    expect([...chart.labels]).toEqual(reference.labels.flat());
    expect([...chart.counts].reduce((a, b) => a + b, 0)).toBe(chart.tilesX * chart.tilesY);
    expect(chart.stencilNames).toHaveLength(reference.palette.length);
    for (const name of chart.stencilNames) expect(name).toMatch(/^stencil-\d{2}-[0-9a-f]{6}\.stl$/);
  });

  it('writes files of the newest chart', () => {
    const png = ask({
      kind: 'chartFile',
      format: 'png',
      gridColour: 0,
      withLines: true
    });
    expect([...png.subarray(0, 4)]).toEqual([0x89, 0x50, 0x4e, 0x47]);
    const pdf = ask({
      kind: 'chartFile',
      format: 'pdf',
      gridColour: 0,
      withLines: false
    });
    expect(new TextDecoder().decode(pdf.subarray(0, 5))).toBe('%PDF-');
    const zip = ask({ kind: 'stencilsZip', thicknessMm: 1.2, skip: [] });
    expect([...zip.subarray(0, 4)]).toEqual([0x50, 0x4b, 0x03, 0x04]);
    expect(entries(zip)).toBe(reference?.palette.length);
    const stl = ask({ kind: 'stencil', index: 0, thicknessMm: 1.2 });
    expect(new TextDecoder().decode(stl.subarray(0, 11))).toBe('stencilator');
  });

  it('gives pixel art a tile per pixel, merges paints and skips stencils', () => {
    const info = ask({ kind: 'open', bytes: bytesOf('pixel-art/fox.png') });
    expect([info.width, info.height, info.pixelsX, info.pixelsY]).toEqual([440, 440, 22, 22]);
    const fox = { ...SETTINGS, sizing: 'pixels', tileMm: 10 } as const;
    const chart = ask({ kind: 'chart', settings: { ...fox, merges: [0x9c3d17, 0xe8772e] } });
    expect([chart.tilesX, chart.tilesY]).toEqual([22, 22]);
    expect([...chart.palette]).toEqual([0x2b, 0x23, 0x20, 0xe8, 0x77, 0x2e, 0xff, 0xff, 0xff]);
    expect([...chart.counts]).toEqual([16, 210, 258]);
    expect(chart.stencilNames).toEqual([
      'stencil-01-2b2320.stl',
      'stencil-02-e8772e.stl',
      'stencil-03-ffffff.stl'
    ]);
    expect(entries(ask({ kind: 'stencilsZip', thicknessMm: 1.2, skip: [0xffffff] }))).toBe(2);
    expect(() => ask({ kind: 'chart', settings: { ...fox, merges: [0x9c3d17] } })).toThrow(
      'Merges come in pairs of colours.'
    );
  });

  it('passes on the engine messages, and forgets the old chart', () => {
    expect(() => ask({ kind: 'chart', settings: { ...SETTINGS, canvasWidthMm: -5 } })).toThrow(
      'The canvas width must be between 1 and 10000 mm.'
    );
    expect(() => ask({ kind: 'chartFile', format: 'png', gridColour: 0, withLines: true })).toThrow(
      'There is no chart'
    );
    expect(() => ask({ kind: 'open', bytes: new ArrayBuffer(8) })).toThrow(
      'The image could not be read'
    );
  });
});
