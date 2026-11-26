/** The Web Worker that runs the engine, so the interface never waits on it. */
import { createHandlers } from './handlers';
import type { Envelope, Reply } from './protocol';
import init, * as engine from './wasm/stencilator_wasm.js';
import wasmUrl from './wasm/stencilator_wasm_bg.wasm?url';

interface WorkerScope {
  onmessage: ((event: MessageEvent<Envelope>) => void) | null;
  postMessage(message: Reply, transfer: Transferable[]): void;
}

const scope = globalThis as unknown as WorkerScope;
const message = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);
const ready = init({ module_or_path: wasmUrl }).then(() => createHandlers(engine));

// Requests are answered one at a time, in the order they arrive.
scope.onmessage = ({ data: { id, request } }) => {
  ready.then(
    (handle) => {
      try {
        const { value, transfer } = handle(request);
        scope.postMessage({ id, ok: true, value }, transfer);
      } catch (error) {
        scope.postMessage({ id, ok: false, message: message(error) }, []);
      }
    },
    (error: unknown) => {
      scope.postMessage(
        { id, ok: false, message: `The engine could not start: ${message(error)}` },
        []
      );
    }
  );
};
