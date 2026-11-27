/** The open project, live previews, editing history and downloads. */
import { untrack } from 'svelte';
import {
  Engine,
  type ArtworkEdits,
  type ArtworkSource,
  type ChartFormat,
  type ChartSummary,
  type Coat,
  type Ends,
  type FileKind,
  type FontSelection,
  type FreeformStage,
  type FreeformSummary,
  type OpenedProject,
  type PlateDrawing,
  type PlateReport,
  type Point,
  type ProjectFont,
  type ProjectMode,
  type SupportDrawing,
  type SupportEdit,
  type SupportKind,
  type TypedText
} from '../engine/client';
import badgeUrl from '../../../fixtures/svg/badge.svg?no-inline';
import { hexToNumber, rgbToHex } from './colour';
import { save } from './download';
import { History } from './history';
import { latest } from './latest';
import {
  baseName,
  chartFileName,
  platesFileName,
  projectFileName,
  stencilFileName,
  stencilsZipName
} from './names';
import {
  equalSettings,
  fromProjectSettings,
  guessSizing,
  isSkipped,
  sanitise,
  toProjectSettings,
  toggledStencil,
  type Settings
} from './settings';
import { readStored, writeStored } from './storage';
import { text } from './text';

export type View = 'picture' | 'chart' | 'both' | 'stencil' | 'result';
export type Screen = 'bench' | 'files';

interface SupportDraft {
  plate: PlateReport;
  support: SupportDrawing | undefined;
  ends: Ends;
  next: 0 | 1;
}

export interface PrintMeshes {
  plate: Uint8Array;
  frame: Uint8Array | undefined;
  insetMm: number;
}

const SETTINGS_KEY = 'stencilator.settings';
/** Changes closer together than this become one step of undo. */
const RECORD_AFTER_MS = 400;
const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/webp'];
const TYPES: Record<ChartFormat, string> = {
  png: 'image/png',
  jpeg: 'image/jpeg',
  pdf: 'application/pdf'
};

interface Edit {
  settings: Settings;
  typed: TypedText | undefined;
  fonts: FontSelection;
}

interface Build extends Edit {
  artwork: ArtworkSource;
  serial: number;
}

interface Loaded {
  project: OpenedProject;
  file?: File;
  kind?: FileKind;
}

const message = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

const sameEdit = (a: Edit, b: Edit): boolean =>
  equalSettings(a.settings, b.settings) && a.typed === b.typed && a.fonts === b.fonts;

const fittedToArtwork = ({ settings }: Edit): boolean =>
  settings.registrationClearanceMm !== null ||
  settings.paintOrder.length > 0 ||
  settings.supportEdits.length > 0;

export class Workbench {
  readonly #engine = new Engine();
  readonly #build = latest(async (input: Build) => {
    // An open may have replaced the project while this build was waiting.
    if (this.opening || input.artwork !== this.artwork) return;
    const settings = toProjectSettings(input.settings);
    if (settings.mode === 'tiles') {
      return { mode: settings.mode, summary: await this.#engine.chart(settings) };
    }
    const summary = await this.#engine.freeform(settings, this.#edits(input), (stage) => {
      if (this.#waitingFor(input)) this.progress = stage;
    });
    return { mode: settings.mode, summary };
  });
  readonly #draw = latest((index: number) => this.#engine.plateDrawing(index));
  readonly #paths = latest<undefined, Coat[]>(() => this.#engine.paintPaths());
  #history: History<Edit>;
  #savedEdit: Edit | undefined;
  #recordTimer: ReturnType<typeof setTimeout> | undefined;
  #buildSerial = 0;
  #nextFontRevision = 0;
  #follow: string | undefined;
  #buildProblem = $state('');
  #actionProblem = $state('');
  #drawn = $state.raw<{ forPlate: PlateReport; drawing: PlateDrawing }>();
  #paintedPaths = $state.raw<{ forBuild: FreeformSummary; coats: Coat[] }>();

  settings: Settings = $state(sanitise(readStored(SETTINGS_KEY)));
  artwork = $state.raw<ArtworkSource>();
  typed = $state.raw<TypedText>();
  fonts: FontSelection = $state.raw({ revision: 0, files: [] });
  pictureUrl = $state('');
  chart = $state.raw<ChartSummary>();
  freeform = $state.raw<FreeformSummary>();
  /** Whether the newest settings have been answered, with a build or a problem. */
  current = $state(false);
  /** Whether the displayed build is the engine's newest and can be exported. */
  valid = $state(false);
  opening = $state(false);
  progress = $state<FreeformStage>();
  saving = $state('');
  view: View = $state('chart');
  plateView = $state<'top' | '3d'>('top');
  comparePaint = $state(false);
  screen: Screen = $state('bench');
  selected = $state(0);
  #support = $state.raw<SupportDrawing>();
  supportDraft = $state.raw<SupportDraft>();
  supportProblem = $state('');
  checkingSupport = $state(false);
  canUndo = $state(false);
  canRedo = $state(false);
  version = $state('');
  minWebMm = $state(0.6);
  #projectName = $state('');

  readonly isFreeform = $derived(this.settings.projectMode === 'freeform');
  readonly source = $derived(this.artwork?.kind === 'picture' ? this.artwork.info : undefined);
  readonly fileName = $derived(this.artwork?.kind === 'text' ? '' : (this.artwork?.name ?? ''));
  readonly base = $derived(baseName(this.fileName));
  readonly projectName = $derived(this.#projectName || projectFileName(this.base));
  readonly problem = $derived(this.#actionProblem || this.#buildProblem);
  readonly canShowFiles = $derived((this.isFreeform ? this.freeform : this.chart) !== undefined);
  readonly canOpen = $derived(!this.opening && !this.saving);
  readonly ready = $derived(this.canShowFiles && this.current && this.valid && this.canOpen);
  readonly canSaveProject = $derived(this.artwork !== undefined && this.canOpen);
  readonly stencilsPossible = $derived(
    this.isFreeform
      ? this.freeform !== undefined
      : this.chart !== undefined && this.chart.linePx > 0
  );
  readonly colours = $derived(paletteHex(this.isFreeform ? this.freeform : this.chart));
  /** Paints that get no stencil, in either mode. */
  readonly unstencilled = $derived(
    this.colours.filter((colour) => isSkipped(this.settings, colour))
  );
  readonly stencilCount = $derived(this.colours.length - this.unstencilled.length);
  readonly selectedPlate = $derived.by(() => {
    if (!this.isFreeform || this.unstencilled.includes(this.colours[this.selected] ?? '')) return;
    return this.freeform?.plates.find((plate) => plate.paint === this.selected);
  });
  readonly drawing = $derived(
    this.#drawn?.forPlate === this.selectedPlate ? this.#drawn?.drawing : undefined
  );
  readonly coatPaths = $derived(
    this.#paintedPaths?.forBuild === this.freeform ? this.#paintedPaths?.coats : undefined
  );
  readonly supportKind = $derived<SupportKind | 'none'>(
    this.settings.raised ? 'raised' : this.settings.bridgeWidthMm !== null ? 'flat' : 'none'
  );
  readonly selectedSupport = $derived(
    this.#support && this.drawing?.supports?.includes(this.#support) ? this.#support : undefined
  );

  constructor() {
    this.#history = new History(this.#snapshot(), sameEdit);
    void this.#engine.about().then(
      (about) => {
        this.version = about.version;
        this.minWebMm = about.minWebMm;
      },
      (error: unknown) => {
        this.#actionProblem = message(error);
      }
    );

    $effect(() => {
      const now = this.#snapshot();
      // Coordinates, paint order and a fitted frame belong to this artwork.
      writeStored(SETTINGS_KEY, {
        ...now.settings,
        registrationClearanceMm: null,
        paintOrder: [],
        supportEdits: []
      });
      clearTimeout(this.#recordTimer);
      this.#recordTimer = setTimeout(() => {
        this.#recordTimer = undefined;
        this.#history.record(now);
        this.#syncHistory();
      }, RECORD_AFTER_MS);
    });

    $effect(() => {
      const artwork = this.artwork;
      if (!artwork || this.opening) return;
      const edit = this.#snapshot();
      // A failed open leaves any finished build intact.
      if (untrack(() => this.current)) return;
      const input: Build = { ...edit, artwork, serial: ++this.#buildSerial };
      this.progress = input.settings.projectMode === 'freeform' ? 'paints' : undefined;
      void this.#build(input).then(
        (outcome) => {
          if (outcome.superseded || !outcome.result || !this.#waitingFor(input)) return;
          const { mode, summary } = outcome.result;
          const colour = this.#follow ?? this.colours[this.selected];
          this.#follow = undefined;
          if (mode === 'tiles') this.chart = summary;
          else this.freeform = summary;
          const match = colour ? this.colours.indexOf(colour) : -1;
          if (match >= 0) this.selected = match;
          else {
            const previous = Math.min(this.selected, this.colours.length - 1);
            this.selected =
              mode === 'freeform' && !summary.plates.some((plate) => plate.paint === previous)
                ? (summary.plates[0]?.paint ?? previous)
                : previous;
          }
          this.#buildProblem = '';
          this.progress = undefined;
          this.valid = true;
          this.current = true;
        },
        (error: unknown) => {
          if (!this.#waitingFor(input)) return;
          // Keep the last drawing or chart in view, faded, while the person corrects it.
          this.#buildProblem = message(error);
          this.progress = undefined;
          this.valid = false;
          this.current = true;
        }
      );
    });

    $effect(() => {
      const build = this.freeform;
      if (
        !this.isFreeform ||
        !build ||
        this.coatPaths ||
        !this.ready ||
        this.screen !== 'bench' ||
        !(this.view === 'result' || (this.view === 'stencil' && this.comparePaint))
      )
        return;
      void this.#paths(undefined).then(
        (outcome) => {
          if (!outcome.superseded && this.freeform === build && this.ready) {
            this.#paintedPaths = { forBuild: build, coats: outcome.result };
          }
        },
        (error: unknown) => {
          if (this.freeform === build && this.ready) this.#actionProblem = message(error);
        }
      );
    });

    $effect(() => {
      const plate = this.selectedPlate;
      if (
        !plate ||
        this.drawing ||
        (this.supportKind === 'none' && (this.view !== 'stencil' || this.plateView !== 'top')) ||
        this.screen !== 'bench' ||
        this.opening ||
        !this.current ||
        !this.valid
      )
        return;
      const index = this.freeform?.plates.indexOf(plate) ?? -1;
      if (index < 0) return;
      void this.#draw(index).then(
        (outcome) => {
          if (!outcome.superseded && this.selectedPlate === plate) {
            this.#drawn = { forPlate: plate, drawing: outcome.result };
          }
        },
        (error: unknown) => {
          if (this.current && this.valid && this.selectedPlate === plate && !this.opening) {
            this.#actionProblem = message(error);
          }
        }
      );
    });
  }

  #snapshot(): Edit {
    return { settings: $state.snapshot(this.settings), typed: this.typed, fonts: this.fonts };
  }

  #edits(edit: Edit): ArtworkEdits {
    return edit.typed ? { typed: edit.typed, fonts: edit.fonts } : { fonts: edit.fonts };
  }

  #waitingFor(input: Build): boolean {
    return !this.opening && this.artwork === input.artwork && this.#buildSerial === input.serial;
  }

  #changing(): void {
    this.#actionProblem = '';
    this.current = false;
    this.#buildSerial++;
    this.cancelSupport();
  }

  change = <K extends keyof Settings>(key: K, value: Settings[K]): void => {
    if (Object.is(this.settings[key], value)) return;
    this.#changing();
    this.settings[key] = value;
  };

  chooseMode = (mode: ProjectMode): void => {
    if (mode === 'tiles' && this.artwork && !this.source) return;
    this.change('projectMode', mode);
    if (mode === 'tiles' && this.view === 'result') this.view = 'chart';
  };

  setSupportKind = (kind: 'none' | SupportKind): void => {
    if (this.supportKind === kind) return;
    this.#changing();
    this.settings.raised = kind === 'raised';
    this.settings.bridgeWidthMm = kind === 'flat' ? 1.2 : null;
    if (kind === 'raised' && Number.isFinite(this.settings.thicknessMm)) {
      this.settings.connectorClearanceMm = Math.max(
        this.settings.connectorClearanceMm,
        Math.ceil(this.settings.thicknessMm * 10 + 2) / 10
      );
    }
  };

  changeText = <K extends keyof TypedText>(key: K, value: TypedText[K]): void => {
    if (!this.typed || Object.is(this.typed[key], value)) return;
    this.#changing();
    this.typed = { ...this.typed, [key]: value };
  };

  builtinFont = (name: string): void => {
    const [current] = this.fonts.files;
    if (current?.name === name && !current.data) return;
    this.#replaceFonts([{ name }]);
  };

  addFont = (file: File): void => {
    const fonts =
      this.artwork?.kind === 'svg' ? this.fonts.files.filter(({ name }) => name !== file.name) : [];
    this.#replaceFonts([...fonts, { name: file.name, data: file }]);
  };

  clearFonts = (): void => {
    this.#replaceFonts([]);
  };

  #replaceFonts(files: readonly ProjectFont[]): void {
    this.#changing();
    this.fonts = { revision: ++this.#nextFontRevision, files };
  }

  select = (index: number): void => {
    if (this.selected !== index) this.cancelSupport();
    this.selected = index;
  };

  show = (view: View): void => {
    if (view !== 'stencil') this.cancelSupport();
    this.view = view;
  };

  showPlate = (view: 'top' | '3d'): void => {
    if (view !== 'top') this.cancelSupport();
    this.plateView = view;
  };

  go = (screen: Screen): void => {
    if (screen !== 'bench') this.cancelSupport();
    this.screen = screen;
  };

  moveCoat = (offset: -1 | 1): void => {
    if (!this.ready || !this.isFreeform || !this.selectedPlate) return;
    const order = this.freeform?.plates.map((plate) => plate.colour) ?? [];
    const i = order.indexOf(this.selectedPlate.colour);
    const j = i + offset;
    const other = order[j];
    if (i < 0 || other === undefined) return;
    order[j] = this.selectedPlate.colour;
    order[i] = other;
    // A skipped paint can be switched back on later.
    this.change(
      'paintOrder',
      [...order, ...this.settings.paintOrder.filter((c) => !order.includes(c))].slice(0, 256)
    );
  };

  selectSupport = (support: SupportDrawing): void => {
    if (!this.ready || this.supportDraft) return;
    this.#support = support;
  };

  editSupport = (support?: SupportDrawing): void => {
    const plate = this.selectedPlate;
    if (!this.ready || !plate || this.supportKind === 'none') return;
    this.screen = 'bench';
    this.view = 'stencil';
    this.plateView = 'top';
    this.supportProblem = '';
    this.supportDraft = {
      plate,
      support,
      ends: support?.ends ?? [
        [NaN, NaN],
        [NaN, NaN]
      ],
      next: 0
    };
  };

  setSupportEnd = (point: Point): void => {
    const draft = this.supportDraft;
    if (!draft || this.checkingSupport) return;
    const ends: Ends = [...draft.ends];
    ends[draft.next] = point;
    this.supportProblem = '';
    this.supportDraft = { ...draft, ends, next: draft.next === 0 ? 1 : 0 };
  };

  setSupportCoordinate = (end: 0 | 1, axis: 0 | 1, mm: number): void => {
    const draft = this.supportDraft;
    if (!draft || this.checkingSupport) return;
    const ends: Ends = [...draft.ends];
    ends[end] = [...ends[end]];
    ends[end][axis] = Math.round(mm * 1000);
    this.supportProblem = '';
    this.supportDraft = { ...draft, ends };
  };

  cancelSupport = (): void => {
    this.supportDraft = undefined;
    this.supportProblem = '';
    this.checkingSupport = false;
  };

  applySupport = async (): Promise<void> => {
    const draft = this.supportDraft;
    if (!draft || !this.ready || this.checkingSupport || this.selectedPlate !== draft.plate) return;
    if (!draft.ends.flat().every((v) => Number.isInteger(v) && Math.abs(v) <= 2 ** 30)) {
      this.supportProblem = text.supports.bothEnds;
      return;
    }
    const index = this.freeform?.plates.indexOf(draft.plate) ?? -1;
    if (index < 0) return;
    this.checkingSupport = true;
    try {
      const issue = await this.#engine.supportIssue(index, draft.ends, draft.support?.edit);
      // Leaving this edit or changing the project clears the draft.
      if (this.supportDraft !== draft) return;
      if (issue) this.supportProblem = text.supports.issues[issue];
      else this.#storeSupport(draft.support, draft.ends);
    } catch (error) {
      if (this.supportDraft === draft) this.supportProblem = message(error);
    } finally {
      if (this.supportDraft === draft) this.checkingSupport = false;
    }
  };

  removeSupport = (support: SupportDrawing): void => {
    if (this.ready && this.drawing?.supports?.includes(support)) this.#storeSupport(support);
  };

  restoreSupports = (): void => {
    const paint = this.selectedPlate?.colour;
    if (!this.ready || !paint) return;
    this.#commitSupportEdits(this.settings.supportEdits.filter((edit) => edit.paint !== paint));
  };

  #storeSupport(support: SupportDrawing | undefined, ends?: Ends): void {
    const paint = this.selectedPlate?.colour;
    const kind = this.supportKind;
    if (!paint || kind === 'none') return;
    const edits = $state.snapshot(this.settings.supportEdits);
    const i = support?.edit;
    const original = i === undefined ? support?.ends : edits[i]?.original;
    const next: SupportEdit = {
      paint,
      kind,
      ...(original ? { original } : {}),
      ...(ends ? { ends } : {})
    };
    if (i !== undefined) edits.splice(i, 1, ...(original || ends ? [next] : []));
    else edits.push(next);
    if (edits.length > 512) {
      this.supportProblem = text.supports.tooMany;
      return;
    }
    this.#commitSupportEdits(edits);
  }

  #commitSupportEdits(edits: SupportEdit[]): void {
    this.#flush();
    this.#history.record(this.#snapshot());
    this.change('supportEdits', edits);
    this.#support = undefined;
    this.#history.record(this.#snapshot());
    this.#syncHistory();
  }

  merge = (from: string, to: string): void => {
    this.#follow = to;
    this.change('merges', [...this.settings.merges, [from, to]]);
  };

  unmerge = (index: number): void => {
    this.#follow = this.settings.merges[index]?.[0];
    this.change(
      'merges',
      this.settings.merges.filter((_, i) => i !== index)
    );
  };

  toggleStencil = (colour: string): void => {
    this.#changing();
    Object.assign(this.settings, toggledStencil(this.settings, colour));
  };

  open = async (file: File): Promise<void> => {
    const kind = fileKind(file);
    if (!kind) {
      this.#actionProblem = text.errors.notArtwork;
      return;
    }
    await this.#openSource(async () => ({
      project: await this.#engine.open(file, kind),
      file,
      kind
    }));
  };

  typeText = (): Promise<void> =>
    this.#openSource(async () => ({
      project: await this.#engine.typeText(text.freeform.initialText)
    }));

  openExample = (): Promise<void> =>
    this.#openSource(async () => {
      const kind = this.isFreeform ? 'svg' : 'picture';
      const response = await fetch(kind === 'svg' ? badgeUrl : './examples/fox.png');
      if (!response.ok) throw new Error(text.errors.example);
      const file = new File([await response.blob()], kind === 'svg' ? 'badge.svg' : 'fox.png');
      return { project: await this.#engine.open(file, kind), file, kind };
    });

  async #openSource(load: () => Promise<Loaded>): Promise<void> {
    if (!this.canOpen) {
      this.#actionProblem = text.project.openBusy;
      return;
    }
    if (
      this.#savedEdit &&
      !sameEdit(this.#savedEdit, this.#snapshot()) &&
      !window.confirm(text.project.replaceUnsaved)
    )
      return;

    this.opening = true;
    this.cancelSupport();
    this.#actionProblem = '';
    try {
      const { project, file, kind } = await load();
      const { source, pictureBytes, fonts } = project;
      const keepPictureView =
        kind === 'picture' &&
        (!this.artwork || this.artwork.kind === 'picture') &&
        this.fonts.files.length === 0;
      if (keepPictureView) this.#flush();
      else clearTimeout(this.#recordTimer);
      // Undo changes settings, not the file. Do not bring back a fit or support
      // that belongs to another picture, even if it was already undone.
      const keepHistory = keepPictureView && !this.#history.some(fittedToArtwork);

      if (this.pictureUrl) URL.revokeObjectURL(this.pictureUrl);
      const picture = pictureBytes ? new Blob([pictureBytes.slice()]) : file;
      this.pictureUrl = source.kind === 'picture' && picture ? URL.createObjectURL(picture) : '';
      this.#projectName = kind === 'project' ? (file?.name ?? '') : '';
      this.chart = undefined;
      this.freeform = undefined;
      this.#drawn = undefined;
      this.#paintedPaths = undefined;
      this.#support = undefined;
      this.valid = false;
      this.current = false;
      this.selected = 0;
      this.#follow = undefined;
      this.#buildProblem = '';
      this.screen = 'bench';
      this.plateView = 'top';
      this.comparePaint = false;
      if (!keepPictureView) this.view = 'chart';

      if (kind === 'project') this.settings = fromProjectSettings(project.settings);
      else {
        this.settings.merges = [];
        this.settings.skipped = [];
        this.settings.stencilCanvasColour = false;
        this.settings.registrationClearanceMm = null;
        this.settings.paintOrder = [];
        this.settings.supportEdits = [];
        if (source.kind === 'picture') this.settings.sizing = guessSizing(source.info);
        else this.settings.sizeMode = 'natural';
        if (source.kind === 'text') {
          this.settings.mode = 'colour';
          this.settings.colours = Math.max(2, this.settings.colours);
        }
      }
      if (source.kind !== 'picture') this.settings.projectMode = 'freeform';
      else if (this.settings.sizeMode === 'natural') this.settings.sizeMode = 'width';

      this.artwork = source;
      this.typed = source.kind === 'text' ? source.style : undefined;
      this.fonts = keepPictureView ? this.fonts : { revision: 0, files: fonts };
      const initial = this.#snapshot();
      this.#savedEdit = initial;
      if (!keepHistory) {
        this.#recordTimer = undefined;
        this.#history = new History(initial, sameEdit);
        this.#syncHistory();
      }
    } catch (error) {
      this.#actionProblem = message(error);
    } finally {
      this.opening = false;
    }
  }

  undo = (): void => {
    if (this.opening) return;
    this.#flush();
    this.#restore(this.#history.undo());
  };

  redo = (): void => {
    if (this.opening) return;
    this.#flush();
    this.#restore(this.#history.redo());
  };

  #restore(edit: Edit | undefined): void {
    if (edit) {
      this.#changing();
      this.settings = { ...edit.settings };
      if (this.settings.projectMode === 'tiles' && this.view === 'result') this.view = 'chart';
      this.typed = edit.typed;
      this.fonts = edit.fonts;
    }
    this.#syncHistory();
  }

  /** Records a change still waiting for its pause, so that undo starts from it. */
  #flush(): void {
    if (this.#recordTimer === undefined) return;
    clearTimeout(this.#recordTimer);
    this.#recordTimer = undefined;
    this.#history.record(this.#snapshot());
  }

  #syncHistory(): void {
    this.canUndo = this.#history.canUndo;
    this.canRedo = this.#history.canRedo;
  }

  saveChart = (format: ChartFormat, withLines: boolean): Promise<void> => {
    if (!this.ready || this.isFreeform) return Promise.resolve();
    const name = chartFileName(this.base, format, withLines);
    const grid = hexToNumber(this.settings.gridColour);
    return this.#save(name, TYPES[format], () => this.#engine.chartFile(format, grid, withLines));
  };

  saveStencil = (index: number): Promise<void> => {
    const engineName = this.chart?.stencilNames[index];
    if (!this.ready || this.isFreeform || !engineName) return Promise.resolve();
    return this.#save(stencilFileName(this.base, engineName), 'model/stl', () =>
      this.#engine.stencil(index, this.settings.thicknessMm)
    );
  };

  saveStencils = (): Promise<void> => {
    if (!this.ready || this.isFreeform) return Promise.resolve();
    const skip = this.unstencilled.map(hexToNumber);
    return this.#save(stencilsZipName(this.base), 'application/zip', () =>
      this.#engine.stencilsZip(this.settings.thicknessMm, skip)
    );
  };

  savePlates = (format: '3mf' | 'zip'): Promise<void> => {
    if (!this.ready || !this.isFreeform || !this.freeform?.plates.length) return Promise.resolve();
    const threeMf = format === '3mf';
    return this.#save(
      platesFileName(this.base, format),
      threeMf ? 'model/3mf' : 'application/zip',
      () => (threeMf ? this.#engine.threeMf() : this.#engine.platesZip())
    );
  };

  savePlate = (index: number): Promise<void> => {
    const plate = this.freeform?.plates[index];
    if (!this.ready || !this.isFreeform || !plate) return Promise.resolve();
    return this.#save(stencilFileName(this.base, plate.stencil), 'model/stl', () =>
      this.#engine.plateFile(index)
    );
  };

  saveFrame = (): Promise<void> => {
    const frame = this.freeform?.frame;
    if (!this.ready || !this.isFreeform || !frame) return Promise.resolve();
    return this.#save(stencilFileName(this.base, frame.file), 'model/stl', () =>
      this.#engine.frameFile()
    );
  };

  async printMeshes(plate: PlateReport): Promise<PrintMeshes | undefined> {
    const build = this.freeform;
    if (!this.ready || !build) return;
    const index = build.plates.indexOf(plate);
    if (index < 0) return;
    const serial = this.#buildSerial;
    const [mesh, frame] = await Promise.all([
      this.#engine.plateFile(index),
      build.frame ? this.#engine.frameFile() : undefined
    ]);
    if (serial !== this.#buildSerial || this.opening || this.selectedPlate !== plate) return;
    return {
      plate: mesh,
      frame,
      insetMm: build.frame ? (build.frame.width_mm - plate.width_mm) / 2 : 0
    };
  }

  saveProject = (): Promise<void> => {
    if (!this.canSaveProject) return Promise.resolve();
    const edit = this.#snapshot();
    return this.#save(
      this.projectName,
      'application/json',
      () => this.#engine.projectFile(toProjectSettings(edit.settings), this.#edits(edit)),
      () => {
        this.#savedEdit = edit;
      }
    );
  };

  async #save(
    name: string,
    type: string,
    task: () => Promise<Uint8Array>,
    onSaved?: () => void
  ): Promise<void> {
    this.saving = name;
    this.#actionProblem = '';
    try {
      save(await task(), name, type);
      onSaved?.();
    } catch (error) {
      this.#actionProblem = message(error);
    } finally {
      this.saving = '';
    }
  }
}

function fileKind(file: File): FileKind | undefined {
  const name = file.name.toLowerCase();
  if (name.endsWith('.stencilator')) return 'project';
  if (name.endsWith('.svg') || file.type === 'image/svg+xml') return 'svg';
  if (IMAGE_TYPES.includes(file.type) || /\.(png|jpe?g|webp)$/.test(name)) return 'picture';
  return file.type === '' ? 'picture' : undefined;
}

function paletteHex(summary: ChartSummary | FreeformSummary | undefined): string[] {
  if (!summary) return [];
  const { palette } = summary;
  return Array.from({ length: palette.length / 3 }, (_, i) =>
    rgbToHex(palette[3 * i] ?? 0, palette[3 * i + 1] ?? 0, palette[3 * i + 2] ?? 0)
  );
}
