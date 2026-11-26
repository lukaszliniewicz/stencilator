/** The interface's way to the engine, which runs in a Web Worker. */
import type {
  ChartFormat,
  ChartSettings,
  ChartSummary,
  Envelope,
  Kind,
  Replies,
  Reply,
  RequestOf,
  SourceInfo
} from './protocol';

export type { ChartFormat, ChartSettings, ChartSummary, Mode, SourceInfo } from './protocol';

interface Waiting {
  resolve: (value: Replies[Kind]) => void;
  reject: (error: Error) => void;
}

export class Engine {
  readonly #worker: Worker;
  readonly #waiting = new Map<number, Waiting>();
  #nextId = 1;

  constructor() {
    this.#worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    this.#worker.onmessage = ({ data }: MessageEvent<Reply>) => {
      const waiting = this.#waiting.get(data.id);
      this.#waiting.delete(data.id);
      if (data.ok) waiting?.resolve(data.value);
      else waiting?.reject(new Error(data.message));
    };
    this.#worker.onerror = (event) => {
      event.preventDefault();
      const error = new Error(`The engine stopped: ${event.message}`);
      for (const waiting of this.#waiting.values()) waiting.reject(error);
      this.#waiting.clear();
    };
  }

  #call<K extends Kind>(request: RequestOf<K>, transfer: Transferable[] = []): Promise<Replies[K]> {
    const id = this.#nextId++;
    return new Promise<Replies[K]>((resolve, reject) => {
      this.#waiting.set(id, { resolve: resolve as (value: Replies[Kind]) => void, reject });
      this.#worker.postMessage({ id, request } satisfies Envelope, transfer);
    });
  }

  version(): Promise<string> {
    return this.#call({ kind: 'version' });
  }

  /** Decodes an image. The chart built before it is gone afterwards. */
  async open(file: Blob): Promise<SourceInfo> {
    const bytes = await file.arrayBuffer();
    return this.#call({ kind: 'open', bytes }, [bytes]);
  }

  chart(settings: ChartSettings): Promise<ChartSummary> {
    return this.#call({ kind: 'chart', settings });
  }

  /** A file of the newest chart, with grid lines or with the tiles packed edge to edge. */
  chartFile(format: ChartFormat, gridColour: number, withLines: boolean): Promise<Uint8Array> {
    return this.#call({ kind: 'chartFile', format, gridColour, withLines });
  }

  /** One colour's stencil, as binary STL. */
  stencil(index: number, thicknessMm: number): Promise<Uint8Array> {
    return this.#call({ kind: 'stencil', index, thicknessMm });
  }

  /** Every colour's stencil, as STL files in one zip. */
  stencilsZip(thicknessMm: number): Promise<Uint8Array> {
    return this.#call({ kind: 'stencilsZip', thicknessMm });
  }
}
