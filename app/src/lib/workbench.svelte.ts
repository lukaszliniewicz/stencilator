/** The open project, live previews, editing history and downloads. */
import { untrack } from 'svelte';
import { SvelteMap } from 'svelte/reactivity';
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
import { readStored } from './storage';
import { text } from './text';
import type { ProjectView } from './workspace-store';

export type View = ProjectView['view'];
export type Screen = 'bench' | 'files';
export type Preset = 'pixel' | 'tile' | 'freeform';

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
  profileId: string | null;
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

interface LoadOptions {
  defaults?: Settings;
  preset?: Preset;
  replace?: boolean;
  profileId: string | null;
  resume?: { id: string; revision: number; view: ProjectView };
}

const message = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

const sameEdit = (a: Edit, b: Edit): boolean =>
  equalSettings(a.settings, b.settings) &&
  a.typed === b.typed &&
  a.fonts === b.fonts &&
  a.profileId === b.profileId;

const fittedToArtwork = ({ settings, fonts }: Edit): boolean =>
  fonts.files.length > 0 ||
  settings.merges.length > 0 ||
  settings.skipped.length > 0 ||
  settings.stencilCanvasColour ||
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
  readonly #histories = new SvelteMap<
    string,
    { revision: number; history: History<Edit>; edit: Edit }
  >();
  #recordTimer: ReturnType<typeof setTimeout> | undefined;
  #buildSerial = 0;
  #nextFontRevision = 0;
  #follow: string | undefined;
  #rememberedColour = $state<string | null>(null);
  #buildProblem = $state('');
  #actionProblem = $state('');
  #drawn = $state.raw<{ forPlate: PlateReport; drawing: PlateDrawing }>();
  #paintedPaths = $state.raw<{ forBuild: FreeformSummary; coats: Coat[] }>();

  settings: Settings = $state(sanitise(readStored(SETTINGS_KEY)));
  artwork = $state.raw<ArtworkSource>();
  typed = $state.raw<TypedText>();
  fonts: FontSelection = $state.raw({ revision: 0, files: [] });
  profileId = $state<string | null>(null);
  editVersion = $state(0);
  artworkVersion = $state(0);
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
  bedPart = $state<'frame' | null>(null);
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

  readonly isFreeform = $derived(this.settings.projectMode === 'freeform');
  readonly source = $derived(this.artwork?.kind === 'picture' ? this.artwork.info : undefined);
  readonly fileName = $derived(this.artwork?.kind === 'text' ? '' : (this.artwork?.name ?? ''));
  readonly base = $derived(baseName(this.fileName));
  readonly projectView: ProjectView = $derived({
    view: this.view,
    plateView: this.plateView,
    comparePaint: this.comparePaint,
    selectedColour: this.#rememberedColour,
    bedPart: this.bedPart
  });
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
  readonly supportKind = $derived(this.settings.supportKind);
  readonly supportEndsSet = $derived(
    this.supportDraft?.ends.flat().every((v) => Number.isInteger(v) && Math.abs(v) <= 2 ** 30) ??
      false
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
      if (!this.artwork || this.opening) return;
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
          const colour = this.#follow ?? this.#rememberedColour ?? this.colours[this.selected];
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
          this.#rememberedColour = this.colours[this.selected] ?? null;
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
    return {
      settings: $state.snapshot(this.settings),
      typed: this.typed,
      fonts: this.fonts,
      profileId: this.profileId
    };
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
    this.editVersion++;
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

  applyBed = (width: number, depth: number, profileId: string | null): void => {
    if (
      width === this.settings.bedWidthMm &&
      depth === this.settings.bedDepthMm &&
      profileId === this.profileId
    )
      return;
    this.#changing();
    this.settings.bedWidthMm = width;
    this.settings.bedDepthMm = depth;
    this.profileId = profileId;
  };

  setSupportKind = (kind: 'none' | SupportKind): void => {
    if (this.supportKind === kind) return;
    this.#changing();
    this.settings.supportKind = kind;
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
    this.#rememberedColour = this.colours[index] ?? null;
    this.bedPart = null;
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
    if (!this.supportEndsSet) {
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
    this.#commitSupportEdits(
      this.settings.supportEdits.filter(
        (edit) => edit.paint !== paint || edit.kind !== this.supportKind
      )
    );
  };

  #storeSupport(support: SupportDrawing | undefined, ends?: Ends): void {
    const paint = this.selectedPlate?.colour;
    const kind = this.supportKind;
    if (!paint || kind === 'none') return;
    const edits = $state.snapshot(this.settings.supportEdits);
    const i = support?.edit;
    if (support?.issue === 'suggestion' && i !== undefined) {
      const keptEnds = edits[i]?.ends;
      edits.splice(i, 1, ...(keptEnds ? [{ paint, kind, ends: keptEnds }] : []));
      this.#commitSupportEdits(edits);
      return;
    }
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

  open = (file: File, options: LoadOptions): Promise<boolean> => {
    const kind = fileKind(file);
    if (!kind) {
      this.#actionProblem = text.errors.notArtwork;
      return Promise.resolve(false);
    }
    return this.#openSource(
      async () => ({ project: await this.#engine.open(file, kind), file, kind }),
      options
    );
  };

  typeText = (options: LoadOptions): Promise<boolean> =>
    this.#openSource(
      async () => ({ project: await this.#engine.typeText(text.freeform.initialText) }),
      options
    );

  openExample = (options: LoadOptions): Promise<boolean> =>
    this.#openSource(async () => {
      const kind = options.preset === 'freeform' ? 'svg' : 'picture';
      const response = await fetch(kind === 'svg' ? badgeUrl : './examples/fox.png');
      if (!response.ok) throw new Error(text.errors.example);
      const file = new File([await response.blob()], kind === 'svg' ? 'badge.svg' : 'fox.png');
      return { project: await this.#engine.open(file, kind), file, kind };
    }, options);

  rememberHistory(id: string, revision: number): void {
    this.#flush();
    this.#histories.delete(id);
    this.#histories.set(id, { revision, history: this.#history, edit: this.#snapshot() });
    // History is only for this session; cap how many inactive artwork/font sets it can hold.
    if (this.#histories.size > 12) {
      const oldest = this.#histories.keys().next().value;
      if (oldest !== undefined) this.#histories.delete(oldest);
    }
  }

  forgetHistory(id: string): void {
    this.#histories.delete(id);
  }

  async #openSource(load: () => Promise<Loaded>, options: LoadOptions): Promise<boolean> {
    if (!this.canOpen) {
      this.#actionProblem = text.project.openBusy;
      return false;
    }

    this.opening = true;
    this.cancelSupport();
    this.#actionProblem = '';
    try {
      const { project, file, kind } = await load();
      const { source, pictureBytes, fonts } = project;
      const { resume, replace, preset } = options;
      const remembered = resume && this.#histories.get(resume.id);
      const cached = remembered?.revision === resume?.revision ? remembered : undefined;
      if (resume) this.#histories.delete(resume.id);
      const view = resume?.view ?? (replace ? this.projectView : undefined);
      let keepHistory = false;
      if (replace && kind === 'picture' && this.artwork?.kind === 'picture') {
        this.#flush();
        // Undo leaves the replacement picture in place. Even an undone edit must
        // not bring back paint choices, a fitted frame or supports for the old picture.
        keepHistory = !this.#history.some(fittedToArtwork);
      }

      if (this.pictureUrl) URL.revokeObjectURL(this.pictureUrl);
      const picture = pictureBytes ? new Blob([pictureBytes.slice()]) : file;
      this.pictureUrl = source.kind === 'picture' && picture ? URL.createObjectURL(picture) : '';
      this.chart = undefined;
      this.freeform = undefined;
      this.#drawn = undefined;
      this.#paintedPaths = undefined;
      this.#support = undefined;
      this.valid = false;
      this.current = false;
      this.selected = 0;
      this.#rememberedColour = view?.selectedColour ?? null;
      this.#follow = undefined;
      this.#buildProblem = '';
      this.screen = 'bench';
      this.plateView = view?.plateView ?? 'top';
      this.comparePaint = view?.comparePaint ?? false;
      this.bedPart = view?.bedPart ?? null;
      this.view = view?.view ?? 'chart';

      if (kind === 'project') this.settings = fromProjectSettings(project.settings);
      else {
        if (!replace && options.defaults) this.settings = { ...options.defaults };
        this.settings.merges = [];
        this.settings.skipped = [];
        this.settings.stencilCanvasColour = false;
        this.settings.registrationClearanceMm = null;
        this.settings.paintOrder = [];
        this.settings.supportEdits = [];
        if (source.kind === 'picture') {
          if (!replace) {
            this.settings.sizing =
              preset === 'pixel'
                ? 'pixels'
                : preset === 'tile'
                  ? 'canvas'
                  : guessSizing(source.info);
            if (preset) this.settings.projectMode = preset === 'freeform' ? 'freeform' : 'tiles';
          }
        } else {
          this.settings.sizeMode = 'natural';
          this.settings.projectMode = 'freeform';
        }
        if (source.kind === 'text') {
          this.settings.mode = 'colour';
          this.settings.colours = Math.max(2, this.settings.colours);
        }
      }
      if (this.settings.projectMode === 'tiles' && this.view === 'result') this.view = 'chart';

      this.artwork = source;
      this.artworkVersion++;
      this.editVersion++;
      if (cached) this.settings = { ...cached.edit.settings };
      if (source.kind === 'picture' && this.settings.sizeMode === 'natural')
        this.settings.sizeMode = 'width';
      this.typed = cached ? cached.edit.typed : source.kind === 'text' ? source.style : undefined;
      if (!keepHistory) this.fonts = cached?.edit.fonts ?? { revision: 0, files: fonts };
      this.profileId = options.profileId;
      if (cached) {
        clearTimeout(this.#recordTimer);
        this.#recordTimer = undefined;
        this.#history = cached.history;
        this.#syncHistory();
      } else if (!keepHistory) this.resetHistory();
      return true;
    } catch (error) {
      this.#actionProblem = message(error);
      return false;
    } finally {
      this.opening = false;
    }
  }

  undo = (): void => {
    if (this.opening || !this.artwork) return;
    this.#flush();
    this.#restore(this.#history.undo());
  };

  resetHistory(): void {
    clearTimeout(this.#recordTimer);
    this.#recordTimer = undefined;
    this.#history = new History(this.#snapshot(), sameEdit);
    this.#syncHistory();
  }

  redo = (): void => {
    if (this.opening || !this.artwork) return;
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
      this.profileId = edit.profileId;
    }
    this.#syncHistory();
  }

  /** Records a change still waiting for its pause, so that undo starts from it. */
  #flush(): void {
    if (this.#recordTimer === undefined) return;
    clearTimeout(this.#recordTimer);
    this.#recordTimer = undefined;
    this.#history.record(this.#snapshot());
    this.#syncHistory();
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

  showBedPart = (part: 'frame' | number): void => {
    if (part === 'frame') this.bedPart = 'frame';
    else this.select(part);
  };

  async bedMesh(part: 'frame' | number): Promise<Uint8Array | undefined> {
    if (!this.ready) return;
    const serial = this.#buildSerial;
    const index = this.freeform?.plates.findIndex((plate) => plate.paint === part) ?? -1;
    const mesh =
      part === 'frame'
        ? await this.#engine.frameFile()
        : this.isFreeform
          ? index < 0
            ? undefined
            : await this.#engine.plateFile(index)
          : await this.#engine.stencil(part, this.settings.thicknessMm);
    return !this.opening && serial === this.#buildSerial ? mesh : undefined;
  }

  validateProject = (file: Blob): Promise<ProjectMode | null> => this.#engine.validateProject(file);

  async projectBytes(strict: boolean): Promise<Blob> {
    const edit = this.#snapshot();
    const bytes = await this.#engine.projectFile(
      toProjectSettings(edit.settings),
      this.#edits(edit),
      strict
    );
    return new Blob([bytes.slice()], { type: 'application/json' });
  }

  saveProject = (title: string, file?: Blob): Promise<void> => {
    if (!this.canSaveProject) return Promise.resolve();
    return this.#save(
      projectFileName(title),
      'application/json',
      () => file ?? this.projectBytes(false)
    );
  };

  async #save(
    name: string,
    type: string,
    task: () => Blob | Promise<Uint8Array | Blob>
  ): Promise<void> {
    this.saving = name;
    this.#actionProblem = '';
    try {
      save(await task(), name, type);
    } catch (error) {
      this.#actionProblem = message(error);
    } finally {
      this.saving = '';
    }
  }
}

export function fileKind(file: File): FileKind | undefined {
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
