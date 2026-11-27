/** The interface's way to the engine, which runs in a Web Worker. */
import type {
  About,
  ArtworkEdits,
  ChartFormat,
  ChartSummary,
  Coat,
  Ends,
  Envelope,
  FileKind,
  FreeformStage,
  FreeformSummary,
  Kind,
  OpenedProject,
  PlateDrawing,
  ProjectMode,
  ProjectSettings,
  Replies,
  Reply,
  RequestOf,
  SupportIssue
} from './protocol';
import { text } from '../lib/text';
import { ProjectSaveError } from './protocol';

export { ProjectSaveError };

export type {
  About,
  ArtworkEdits,
  ArtworkSize,
  ArtworkSource,
  BedFit,
  ChartFormat,
  ChartSummary,
  Coat,
  Ends,
  FileKind,
  FontSelection,
  FreeformStage,
  FreeformSummary,
  Mark,
  Mode,
  OpenedProject,
  PlateDrawing,
  PlateReport,
  Point,
  ProjectFont,
  ProjectMode,
  ProjectSettings,
  RasterPreview,
  Sizing,
  SourceInfo,
  SupportDrawing,
  SupportEdit,
  SupportIssue,
  SupportKind,
  TypedText
} from './protocol';

interface Waiting {
  resolve: (value: Replies[Kind]) => void;
  reject: (error: Error) => void;
  progress: ((stage: FreeformStage) => void) | undefined;
}

export class Engine {
  readonly #worker: Worker;
  readonly #waiting = new Map<number, Waiting>();
  #nextId = 1;

  constructor() {
    this.#worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    this.#worker.onmessage = ({ data }: MessageEvent<Reply>) => {
      const waiting = this.#waiting.get(data.id);
      if ('stage' in data) {
        waiting?.progress?.(data.stage);
        return;
      }
      this.#waiting.delete(data.id);
      if (data.ok) waiting?.resolve(data.value);
      else if ('saveProblem' in data) waiting?.reject(new ProjectSaveError(data.saveProblem));
      else waiting?.reject(new Error(data.message));
    };
    this.#worker.onerror = (event) => {
      event.preventDefault();
      const error = new Error(text.errors.stopped(event.message));
      for (const waiting of this.#waiting.values()) waiting.reject(error);
      this.#waiting.clear();
    };
  }

  #call<K extends Kind>(
    request: RequestOf<K>,
    transfer: Transferable[] = [],
    progress?: (stage: FreeformStage) => void
  ): Promise<Replies[K]> {
    const id = this.#nextId++;
    return new Promise<Replies[K]>((resolve, reject) => {
      this.#waiting.set(id, {
        resolve: resolve as (value: Replies[Kind]) => void,
        reject,
        progress
      });
      this.#worker.postMessage({ id, request } satisfies Envelope, transfer);
    });
  }

  about(): Promise<About> {
    return this.#call({ kind: 'about' });
  }

  /** Opens a source or a complete project. A failed open leaves the workspace alone. */
  async open(file: File, kind: FileKind): Promise<OpenedProject> {
    const bytes = await file.arrayBuffer();
    return this.#call({ kind: 'open', file: kind, name: file.name, bytes }, [bytes]);
  }

  typeText(value: string): Promise<OpenedProject> {
    return this.#call({ kind: 'typeText', value });
  }

  chart(settings: ProjectSettings): Promise<ChartSummary> {
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

  /** Every colour's stencil but the skipped ones, as STL files in one zip. */
  stencilsZip(thicknessMm: number, skip: number[]): Promise<Uint8Array> {
    return this.#call({ kind: 'stencilsZip', thicknessMm, skip });
  }

  freeform(
    settings: ProjectSettings,
    edits: ArtworkEdits,
    progress: (stage: FreeformStage) => void
  ): Promise<FreeformSummary> {
    return this.#call({ kind: 'freeform', settings, edits }, [], progress);
  }

  plateDrawing(index: number): Promise<PlateDrawing> {
    return this.#call({ kind: 'plateDrawing', index });
  }

  supportIssue(
    index: number,
    ends: Ends,
    edit: number | undefined
  ): Promise<SupportIssue | undefined> {
    return this.#call({ kind: 'supportIssue', index, ends, edit });
  }

  paintPaths(): Promise<Coat[]> {
    return this.#call({ kind: 'paintPaths' });
  }

  plateFile(index: number): Promise<Uint8Array> {
    return this.#call({ kind: 'plateFile', index });
  }

  frameFile(): Promise<Uint8Array> {
    return this.#call({ kind: 'frameFile' });
  }

  platesZip(): Promise<Uint8Array> {
    return this.#call({ kind: 'platesZip' });
  }

  threeMf(): Promise<Uint8Array> {
    return this.#call({ kind: 'threeMf' });
  }

  projectFile(settings: ProjectSettings, edits: ArtworkEdits, strict = false): Promise<Uint8Array> {
    return this.#call({ kind: 'projectFile', settings, edits, strict });
  }

  /** Validates a file and returns the mode the app would open it in, without opening it. */
  async validateProject(file: Blob): Promise<ProjectMode> {
    const bytes = await file.arrayBuffer();
    return this.#call({ kind: 'validateProject', bytes }, [bytes]);
  }
}
