/**
 * The engine side of the protocol: keeps the open image and the newest chart in
 * WebAssembly memory and answers requests about them. Separate from the worker so that
 * tests can drive it under Node with the real WebAssembly build.
 */
import type { Replies, Request } from './protocol';
import type * as Wasm from './wasm/stencilator_wasm.js';

type Engine = Pick<typeof Wasm, 'Settings' | 'Source' | 'version' | 'recommendedMinWebMm'>;

export interface Answer {
  value: Replies[Request['kind']];
  /** Buffers to hand over to the other side instead of copying. */
  transfer: Transferable[];
}

export function createHandlers(engine: Engine): (request: Request) => Answer {
  let source: Wasm.Source | undefined;
  let chart: Wasm.Chart | undefined;

  const openSource = (): Wasm.Source => {
    if (!source) throw new Error('Open an image first.');
    return source;
  };
  const builtChart = (): Wasm.Chart => {
    if (!chart) throw new Error('There is no chart for these settings.');
    return chart;
  };
  const file = (bytes: Uint8Array): Answer => ({ value: bytes, transfer: [bytes.buffer] });

  return (request) => {
    switch (request.kind) {
      case 'about':
        return {
          value: { version: engine.version(), minWebMm: engine.recommendedMinWebMm() },
          transfer: []
        };
      case 'open': {
        const next = new engine.Source(new Uint8Array(request.bytes));
        chart?.free();
        chart = undefined;
        source?.free();
        source = next;
        return { value: { width: next.width, height: next.height }, transfer: [] };
      }
      case 'chart': {
        // Until the new chart exists, no file may come from the old one.
        chart?.free();
        chart = undefined;
        const s = request.settings;
        const settings = new engine.Settings();
        try {
          settings.canvasWidthMm = s.canvasWidthMm;
          settings.canvasHeightMm = s.canvasHeightMm;
          settings.tileMm = s.tileMm;
          settings.lineMm = s.lineMm;
          settings.dpi = s.dpi;
          settings.mode = s.mode;
          settings.colours = s.colours;
          settings.background = s.background;
          chart = openSource().chart(settings);
        } finally {
          settings.free();
        }
        const summary = {
          tilesX: chart.tilesX,
          tilesY: chart.tilesY,
          tilePx: chart.tilePx,
          linePx: chart.linePx,
          tileMm: chart.tileMm,
          lineMm: chart.lineMm,
          widthMm: chart.widthMm,
          heightMm: chart.heightMm,
          widthPx: chart.widthPx,
          heightPx: chart.heightPx,
          palette: chart.palette(),
          labels: chart.labels(),
          counts: chart.tileCounts(),
          stencilNames: chart.stencilNames()
        };
        return {
          value: summary,
          transfer: [summary.palette.buffer, summary.labels.buffer, summary.counts.buffer]
        };
      }
      case 'chartFile':
        return file(builtChart().chartFile(request.format, request.gridColour, request.withLines));
      case 'stencil':
        return file(builtChart().stencil(request.index, request.thicknessMm));
      case 'stencilsZip':
        return file(builtChart().stencilsZip(request.thicknessMm));
    }
  };
}
