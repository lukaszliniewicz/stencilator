/**
 * The state of the workspace: the open picture, the settings with their history, the
 * newest chart, and saving files from it. Build it while a component initialises, since
 * it keeps the chart in step with the settings through effects.
 */
import { Engine, type ChartFormat, type ChartSummary, type SourceInfo } from '../engine/client';
import { hexToNumber, rgbToHex } from './colour';
import { save } from './download';
import { History } from './history';
import { latest } from './latest';
import { baseName, chartFileName, stencilFileName, stencilsZipName } from './names';
import {
  equalSettings,
  guessSizing,
  isSkipped,
  sanitise,
  toChartSettings,
  toggledStencil,
  type Settings
} from './settings';
import { readStored, writeStored } from './storage';
import { text } from './text';

export type View = 'picture' | 'chart' | 'both' | 'stencil';
export type Screen = 'bench' | 'files';

const SETTINGS_KEY = 'stencilator.settings';
/** Changes closer together than this become one step of undo. */
const RECORD_AFTER_MS = 400;
const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/webp'];
const EXAMPLE = 'fox.png';
const TYPES: Record<ChartFormat, string> = {
  png: 'image/png',
  jpeg: 'image/jpeg',
  pdf: 'application/pdf'
};

const message = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export class Workbench {
  readonly #engine = new Engine();
  readonly #build = latest((settings: Settings) => this.#engine.chart(toChartSettings(settings)));
  readonly #history: History<Settings>;
  #recordTimer: ReturnType<typeof setTimeout> | undefined;
  /** A paint to select once the next chart is built, as its palette may move. */
  #follow: string | undefined;

  settings: Settings = $state(sanitise(readStored(SETTINGS_KEY)));
  source: SourceInfo | undefined = $state();
  fileName = $state('');
  pictureUrl = $state('');
  chart: ChartSummary | undefined = $state.raw();
  /** Whether the newest settings have been answered, with a chart or a problem. */
  current = $state(false);
  /** Whether the chart shown is the engine's newest, so that files can come from it. */
  valid = $state(false);
  problem = $state('');
  /** The file being saved, by its name, or nothing. */
  saving = $state('');
  view: View = $state('chart');
  screen: Screen = $state('bench');
  selected = $state(0);
  canUndo = $state(false);
  canRedo = $state(false);
  version = $state('');
  minWebMm = $state(0.6);

  readonly base = $derived(baseName(this.fileName));
  readonly ready = $derived(
    this.chart !== undefined && this.current && this.valid && this.saving === ''
  );
  readonly stencilsPossible = $derived(this.chart !== undefined && this.chart.linePx > 0);
  readonly colours = $derived(paletteHex(this.chart));
  /** The paints of the chart that get no stencil. */
  readonly unstencilled = $derived(
    this.colours.filter((colour) => isSkipped(this.settings, colour))
  );
  readonly stencilCount = $derived(this.colours.length - this.unstencilled.length);

  constructor() {
    this.#history = new History($state.snapshot(this.settings), equalSettings);
    this.#engine.about().then(
      (about) => {
        this.version = about.version;
        this.minWebMm = about.minWebMm;
      },
      (error: unknown) => {
        this.problem = message(error);
      }
    );

    // Remember every change, and make pauses in changing into steps of undo.
    $effect(() => {
      const now = $state.snapshot(this.settings);
      writeStored(SETTINGS_KEY, now);
      clearTimeout(this.#recordTimer);
      this.#recordTimer = setTimeout(() => {
        this.#recordTimer = undefined;
        this.#record(now);
      }, RECORD_AFTER_MS);
    });

    $effect(() => {
      if (!this.source) return;
      this.current = false;
      this.#build($state.snapshot(this.settings)).then(
        (outcome) => {
          if (outcome.superseded) return;
          this.chart = outcome.result;
          const followed = this.#follow ? paletteHex(this.chart).indexOf(this.#follow) : -1;
          this.#follow = undefined;
          this.selected =
            followed >= 0 ? followed : Math.min(this.selected, outcome.result.counts.length - 1);
          this.problem = '';
          this.valid = true;
          this.current = true;
        },
        (error: unknown) => {
          // The last chart stays in view, faded; the engine no longer holds it.
          this.problem = message(error);
          this.valid = false;
          this.current = true;
        }
      );
    });
  }

  change = <K extends keyof Settings>(key: K, value: Settings[K]): void => {
    this.settings[key] = value;
  };

  select = (index: number): void => {
    this.selected = index;
  };

  show = (view: View): void => {
    this.view = view;
  };

  go = (screen: Screen): void => {
    this.screen = screen;
  };

  /** Gives every tile of one paint to another, which stays selected. */
  merge = (from: string, to: string): void => {
    this.#follow = to;
    this.settings.merges = [...this.settings.merges, [from, to]];
  };

  /** Takes back one merge, selecting the paint it brings back. */
  unmerge = (index: number): void => {
    this.#follow = this.settings.merges[index]?.[0];
    this.settings.merges = this.settings.merges.filter((_, i) => i !== index);
  };

  toggleStencil = (colour: string): void => {
    const next = toggledStencil(this.settings, colour);
    this.settings.skipped = next.skipped;
    this.settings.stencilCanvasColour = next.stencilCanvasColour;
  };

  open = async (file: File): Promise<void> => {
    if (file.type !== '' && !IMAGE_TYPES.includes(file.type)) {
      this.problem = text.errors.notAPicture;
      return;
    }
    try {
      const source = await this.#engine.open(file);
      if (this.pictureUrl) URL.revokeObjectURL(this.pictureUrl);
      this.pictureUrl = URL.createObjectURL(file);
      this.fileName = file.name;
      // The engine has let go of the old chart.
      this.chart = undefined;
      this.valid = false;
      this.selected = 0;
      this.problem = '';
      // Merges and stencils belong to a picture's paints; the sizing is a guess, and
      // undoing takes it back.
      this.settings.sizing = guessSizing(source);
      this.settings.merges = [];
      this.settings.skipped = [];
      this.settings.stencilCanvasColour = false;
      this.source = source;
    } catch (error) {
      this.problem = message(error);
    }
  };

  openExample = async (): Promise<void> => {
    try {
      const response = await fetch(`./examples/${EXAMPLE}`);
      if (!response.ok) throw new Error(response.statusText);
      const blob = await response.blob();
      await this.open(new File([blob], EXAMPLE, { type: 'image/png' }));
    } catch {
      this.problem = text.errors.example;
    }
  };

  undo = (): void => {
    this.#flush();
    const previous = this.#history.undo();
    if (previous) this.settings = { ...previous };
    this.#sync();
  };

  redo = (): void => {
    this.#flush();
    const next = this.#history.redo();
    if (next) this.settings = { ...next };
    this.#sync();
  };

  saveChart = (format: ChartFormat, withLines: boolean): Promise<void> => {
    const name = chartFileName(this.base, format, withLines);
    const grid = hexToNumber(this.settings.gridColour);
    return this.#save(name, TYPES[format], () => this.#engine.chartFile(format, grid, withLines));
  };

  saveStencil = (index: number): Promise<void> => {
    const engineName = this.chart?.stencilNames[index];
    if (engineName === undefined) return Promise.resolve();
    const thickness = this.settings.thicknessMm;
    return this.#save(stencilFileName(this.base, engineName), 'model/stl', () =>
      this.#engine.stencil(index, thickness)
    );
  };

  saveStencils = (): Promise<void> => {
    const thickness = this.settings.thicknessMm;
    const skip = this.unstencilled.map(hexToNumber);
    return this.#save(stencilsZipName(this.base), 'application/zip', () =>
      this.#engine.stencilsZip(thickness, skip)
    );
  };

  async #save(name: string, type: string, task: () => Promise<Uint8Array>): Promise<void> {
    this.saving = name;
    try {
      save(await task(), name, type);
    } catch (error) {
      this.problem = message(error);
    } finally {
      this.saving = '';
    }
  }

  #record(next: Settings): void {
    this.#history.record(next);
    this.#sync();
  }

  /** Records a change still waiting for its pause, so that undo starts from it. */
  #flush(): void {
    if (this.#recordTimer === undefined) return;
    clearTimeout(this.#recordTimer);
    this.#recordTimer = undefined;
    this.#history.record($state.snapshot(this.settings));
  }

  #sync(): void {
    this.canUndo = this.#history.canUndo;
    this.canRedo = this.#history.canRedo;
  }
}

function paletteHex(chart: ChartSummary | undefined): string[] {
  if (!chart) return [];
  return Array.from({ length: chart.palette.length / 3 }, (_, i) =>
    rgbToHex(
      chart.palette[3 * i] ?? 0,
      chart.palette[3 * i + 1] ?? 0,
      chart.palette[3 * i + 2] ?? 0
    )
  );
}
