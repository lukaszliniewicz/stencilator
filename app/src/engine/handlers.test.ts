/// <reference types="node" />
import { readFileSync } from 'node:fs';

import { beforeAll, describe, expect, it } from 'vitest';

import { createHandlers } from './handlers';
import type { ChartSettings, Kind, Replies, RequestOf } from './protocol';
import * as engine from './wasm/stencilator_wasm.js';

const fixture = (name: string) => new URL(`../../../fixtures/reference/${name}`, import.meta.url);

interface ReferenceChart {
  image: string;
  settings: { canvas_mm: [number, number]; tile_mm: number; line_mm: number; dpi: number };
  tiles: [number, number];
  palette: [number, number, number][];
  labels: number[][];
}

const reference = (
  JSON.parse(readFileSync(fixture('charts.json'), 'utf8')) as { cases: ReferenceChart[] }
).cases[0];

const SETTINGS: ChartSettings = {
  canvasWidthMm: 47,
  canvasHeightMm: 40,
  tileMm: 5,
  lineMm: 0.8,
  dpi: 300,
  mode: 'colour',
  colours: 16,
  background: 0xffffff
};

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
    expect(() => ask({ kind: 'stencilsZip', thicknessMm: 1.2 })).toThrow('There is no chart');
  });

  it('builds the reference chart', () => {
    if (!reference) throw new Error('no reference chart');
    const info = ask({ kind: 'open', bytes: bytesOf(reference.image) });
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
    const zip = ask({ kind: 'stencilsZip', thicknessMm: 1.2 });
    expect([...zip.subarray(0, 4)]).toEqual([0x50, 0x4b, 0x03, 0x04]);
    const stl = ask({ kind: 'stencil', index: 0, thicknessMm: 1.2 });
    expect(new TextDecoder().decode(stl.subarray(0, 11))).toBe('stencilator');
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
