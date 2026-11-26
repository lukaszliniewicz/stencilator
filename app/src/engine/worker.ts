/** The Web Worker that runs the engine, so the interface never waits on it. */
import { createHandlers } from './handlers';
import type { Envelope, Reply } from './protocol';
import allertaUrl from './wasm/fonts/AllertaStencil-Regular.ttf?url';
import notoUrl from './wasm/fonts/NotoSans-Bold.ttf?url';
import init, * as engine from './wasm/stencilator_wasm.js';
import wasmUrl from './wasm/stencilator_wasm_bg.wasm?url';
import { text } from '../lib/text';

interface WorkerScope {
  onmessage: ((event: MessageEvent<Envelope>) => void) | null;
  postMessage(message: Reply, transfer: Transferable[]): void;
}

const scope = globalThis as unknown as WorkerScope;
const message = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

async function loadFonts(): Promise<engine.Fonts> {
  const assets = [
    { name: 'NotoSans-Bold.ttf', url: notoUrl },
    { name: 'AllertaStencil-Regular.ttf', url: allertaUrl }
  ];
  const bytes = await Promise.all(
    assets.map(async ({ name, url }) => {
      const response = await fetch(url);
      if (!response.ok) throw new Error(text.errors.builtInFont(name));
      return { name, data: new Uint8Array(await response.arrayBuffer()) };
    })
  );
  const fonts = new engine.Fonts();
  for (const { name, data } of bytes) fonts.add(name, data);
  return fonts;
}

let fonts: Promise<engine.Fonts> | undefined;
const drawingFonts = (): Promise<engine.Fonts> =>
  (fonts ??= loadFonts().catch((error: unknown) => {
    fonts = undefined;
    throw error;
  }));

const ready = init({ module_or_path: wasmUrl }).then(
  () => createHandlers(engine, drawingFonts),
  (error: unknown) => {
    throw new Error(text.errors.start(message(error)));
  }
);
let queue = Promise.resolve();

// Requests are answered one at a time, in the order they arrive.
scope.onmessage = ({ data: { id, request } }) => {
  queue = queue.then(async () => {
    try {
      const handle = await ready;
      const { value, transfer } = await handle(request, (stage) => {
        scope.postMessage({ id, stage }, []);
      });
      scope.postMessage({ id, ok: true, value }, transfer);
    } catch (error) {
      scope.postMessage({ id, ok: false, message: message(error) }, []);
    }
  });
};
