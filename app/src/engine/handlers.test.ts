/// <reference types="node" />
import { readFileSync } from 'node:fs';

import { beforeAll, describe, expect, it } from 'vitest';

import { sanitise, toProjectSettings } from '../lib/settings';
import { createHandlers } from './handlers';
import type { Kind, Replies, RequestOf } from './protocol';
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

const SETTINGS = toProjectSettings({
  ...sanitise(undefined),
  canvasWidthMm: 47,
  canvasHeightMm: 40
});

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
  const ask = async <K extends Kind>(request: RequestOf<K>): Promise<Replies[K]> =>
    (await handle(request)).value as Replies[K];
  const picture = async (name: string) =>
    ask({ kind: 'open', file: 'picture', name, bytes: bytesOf(name) });

  beforeAll(() => {
    engine.initSync({
      module: readFileSync(new URL('wasm/stencilator_wasm_bg.wasm', import.meta.url))
    });
    const fonts = new engine.Fonts();
    for (const name of engine.Fonts.builtIn()) {
      fonts.add(name, readFileSync(new URL(`../../../fonts/${name}`, import.meta.url)));
    }
    handle = createHandlers(engine, () => Promise.resolve(fonts));
  });

  it('tells its version and the thinnest web it recommends', async () => {
    const about = await ask({ kind: 'about' });
    expect(about.version).toMatch(/^\d+\.\d+\.\d+/);
    expect(about.minWebMm).toBeGreaterThan(0);
  });

  it('refuses files before there is a chart', async () => {
    await expect(ask({ kind: 'chart', settings: SETTINGS })).rejects.toThrow('Open artwork first.');
    await expect(ask({ kind: 'stencilsZip', thicknessMm: 1.2, skip: [] })).rejects.toThrow(
      'There is no chart'
    );
  });

  it('builds the reference chart', async () => {
    if (!reference) throw new Error('no reference chart');
    const { source } = await picture(`reference/${reference.image}`);
    expect(source.kind === 'picture' && source.info.width).toBeGreaterThan(0);
    const chart = await ask({ kind: 'chart', settings: SETTINGS });
    expect([chart.tilesX, chart.tilesY]).toEqual(reference.tiles);
    expect([...chart.palette]).toEqual(reference.palette.flat());
    expect([...chart.labels]).toEqual(reference.labels.flat());
    expect([...chart.counts].reduce((a, b) => a + b, 0)).toBe(chart.tilesX * chart.tilesY);
    expect(chart.stencilNames).toHaveLength(reference.palette.length);
    for (const name of chart.stencilNames) expect(name).toMatch(/^stencil-\d{2}-[0-9a-f]{6}\.stl$/);
  });

  it('writes files of the newest chart', async () => {
    const png = await ask({
      kind: 'chartFile',
      format: 'png',
      gridColour: 0,
      withLines: true
    });
    expect([...png.subarray(0, 4)]).toEqual([0x89, 0x50, 0x4e, 0x47]);
    const pdf = await ask({
      kind: 'chartFile',
      format: 'pdf',
      gridColour: 0,
      withLines: false
    });
    expect(new TextDecoder().decode(pdf.subarray(0, 5))).toBe('%PDF-');
    const zip = await ask({ kind: 'stencilsZip', thicknessMm: 1.2, skip: [] });
    expect([...zip.subarray(0, 4)]).toEqual([0x50, 0x4b, 0x03, 0x04]);
    expect(entries(zip)).toBe(reference?.palette.length);
    const stl = await ask({ kind: 'stencil', index: 0, thicknessMm: 1.2 });
    expect(new TextDecoder().decode(stl.subarray(0, 11))).toBe('stencilator');
  });

  it('gives pixel art a tile per pixel, merges paints and skips stencils', async () => {
    const { source } = await picture('pixel-art/fox.png');
    if (source.kind !== 'picture') throw new Error('no picture');
    const info = source.info;
    expect([info.width, info.height, info.pixelsX, info.pixelsY]).toEqual([440, 440, 22, 22]);
    const fox = toProjectSettings({
      ...sanitise(undefined),
      sizing: 'pixels',
      tileMm: 10,
      merges: [['#9c3d17', '#e8772e']]
    });
    const chart = await ask({ kind: 'chart', settings: fox });
    expect([chart.tilesX, chart.tilesY]).toEqual([22, 22]);
    expect([...chart.palette]).toEqual([0x2b, 0x23, 0x20, 0xe8, 0x77, 0x2e, 0xff, 0xff, 0xff]);
    expect([...chart.counts]).toEqual([16, 210, 258]);
    expect(chart.stencilNames).toEqual([
      'stencil-01-2b2320.stl',
      'stencil-02-e8772e.stl',
      'stencil-03-ffffff.stl'
    ]);
    expect(entries(await ask({ kind: 'stencilsZip', thicknessMm: 1.2, skip: [0xffffff] }))).toBe(2);
  });

  it('checks tile stencils against the saved bed without changing the mesh', async () => {
    await picture('pixel-art/fox.png');
    const fox = toProjectSettings({ ...sanitise(undefined), sizing: 'pixels', tileMm: 20 });
    const build = (bed: [number, number]) =>
      ask({ kind: 'chart', settings: { ...fox, stencils: { ...fox.stencils, bed_mm: bed } } });
    const stencil = () => ask({ kind: 'stencil', index: 0, thicknessMm: 1.2 });

    const largeBed = await build([500, 500]);
    expect(largeBed.widthMm).toBeGreaterThan(256);
    expect(largeBed.bedFit).toBe('as-drawn');
    const mesh = await stencil();
    expect((await build([50, 50])).bedFit).toBe('too-large');
    expect(await stencil()).toEqual(mesh);
    expect((await build([-1, 500])).bedFit).toBeUndefined();
  });

  it('passes on the engine messages, and forgets the old chart', async () => {
    await expect(
      ask({
        kind: 'chart',
        settings: { ...SETTINGS, tiles: { ...SETTINGS.tiles, canvas_mm: [-5, 40] } }
      })
    ).rejects.toThrow('The canvas width must be between 1 and 10000 mm.');
    await expect(
      ask({ kind: 'chartFile', format: 'png', gridColour: 0, withLines: true })
    ).rejects.toThrow('There is no chart');
    await expect(
      ask({ kind: 'open', file: 'picture', name: 'bad.png', bytes: new ArrayBuffer(8) })
    ).rejects.toThrow('The image could not be read');
  });

  it('saves edits to a native project without rounding a 64-bit seed', async () => {
    const original = readFileSync(fixture('projects/fox.stencilator'), 'utf8');
    const seed = '18446744073709551615';
    const bytes = new TextEncoder().encode(original.replace(/"seed": \d+/, `"seed": ${seed}`));
    const { settings, fonts } = await ask({
      kind: 'open',
      file: 'project',
      name: 'fox.stencilator',
      bytes: bytes.buffer
    });
    const edited = {
      ...settings,
      mode: 'tiles',
      freeform: { ...settings.freeform, size: { width_mm: 190 } }
    } as const;
    await ask({ kind: 'chart', settings: edited });
    const file = await ask({
      kind: 'projectFile',
      settings: edited,
      edits: { fonts: { revision: 0, files: fonts } }
    });
    const saved = new TextDecoder().decode(file);
    expect(saved).toContain(`"seed": ${seed}`);
    expect(saved).toContain('"mode": "tiles"');
    expect(saved).toContain('"width_mm": 190');
  });

  it('saves a snapshot without disturbing the active preview or its exports', async () => {
    await picture('pixel-art/fox.png');
    const small = toProjectSettings({ ...sanitise(undefined), sizing: 'pixels', tileMm: 10 });
    const large = toProjectSettings({ ...sanitise(undefined), sizing: 'pixels', tileMm: 20 });

    await ask({ kind: 'chart', settings: small });
    const before = await ask({ kind: 'stencil', index: 0, thicknessMm: 1.2 });

    const file = await ask({
      kind: 'projectFile',
      settings: large,
      edits: { fonts: { revision: 0, files: [] } },
      strict: true
    });
    expect(new TextDecoder().decode(file)).toContain('"tile_mm": 20');
    // The live chart and its exports still come from the small settings.
    expect(await ask({ kind: 'stencil', index: 0, thicknessMm: 1.2 })).toEqual(before);
  });

  it('reports save failures and uses the last valid inactive settings for a download', async () => {
    await picture('pixel-art/fox.png');
    const small = toProjectSettings({ ...sanitise(undefined), sizing: 'pixels', tileMm: 10 });
    const large = toProjectSettings({ ...sanitise(undefined), sizing: 'pixels', tileMm: 20 });
    await ask({ kind: 'chart', settings: small });
    const stencil = () => ask({ kind: 'stencil', index: 0, thicknessMm: 1.2 });
    const before = await stencil();
    const inactive = { ...large, freeform: { ...large.freeform, cell_mm: -1 } };
    const active = { ...large, tiles: { ...large.tiles, tile_mm: -1 } };
    const save = (
      settings: typeof large,
      strict: boolean,
      edits: RequestOf<'projectFile'>['edits'] = { fonts: { revision: 0, files: [] } }
    ) => ask({ kind: 'projectFile', settings, edits, strict });

    await expect(save(inactive, true)).rejects.toMatchObject({
      name: 'ProjectSaveError',
      problem: { kind: 'inactive-settings', mode: 'freeform' }
    });
    expect(await stencil()).toEqual(before);

    await expect(save(active, true)).rejects.toMatchObject({
      name: 'ProjectSaveError',
      problem: { kind: 'settings' }
    });
    expect(await stencil()).toEqual(before);

    const saved = new TextDecoder().decode(await save(inactive, false));
    expect(saved).toContain('"tile_mm": 20');
    expect(saved).not.toContain('"cell_mm": -1');
    expect(await stencil()).toEqual(before);

    await expect(
      save(large, true, { fonts: { revision: 1, files: [{ name: 'Missing.ttf' }] } })
    ).rejects.toMatchObject({ name: 'ProjectSaveError', problem: { kind: 'artwork' } });
    expect(await stencil()).toEqual(before);
  });

  it('validates a project file without opening it', async () => {
    expect(await ask({ kind: 'validateProject', bytes: bytesOf('projects/fox.stencilator') })).toBe(
      'freeform'
    );
    const malformed = new TextEncoder().encode('{}').buffer;
    expect(await ask({ kind: 'validateProject', bytes: malformed })).toBeNull();
    const undecodable = new TextEncoder().encode(
      '{"stencilator": 1, "source": {"kind": "picture", "data": "AAAA"}}'
    ).buffer;
    expect(await ask({ kind: 'validateProject', bytes: undecodable })).toBeNull();
  });
});
