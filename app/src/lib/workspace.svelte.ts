/** Keeps the live workbench and its saved browser record in step. */
import { SvelteSet } from 'svelte/reactivity';
import { ProjectSaveError } from '../engine/client';
import { baseName, projectFileName } from './names';
import type { Settings } from './settings';
import { normaliseDefaults } from './settings';
import { readStored } from './storage';
import { text } from './text';
import type { ThemeChoice } from './theme.svelte';
import { type Preset, type Workbench } from './workbench.svelte';
import {
  decodeBackup,
  planBackups,
  previewRestore,
  type BackupPlan,
  type WorkspaceBackup
} from './workspace-backup';
import {
  openWorkspace,
  type PrinterProfile,
  type ProfileWrite,
  type ProjectView,
  type SavedProject,
  type WorkspaceSnapshot,
  type WorkspaceStore,
  WorkspaceError
} from './workspace-store';

const SAVE_AFTER_MS = 450;
export const TITLE_LIMIT = 120;
const words = text.workspace;
const legacyDefaults = (): Settings => normaliseDefaults(readStored('stencilator.settings'));

export interface ReplacementTarget {
  id: string;
  revision: number | null;
}

interface SaveDraft {
  version: number;
  title: string;
  view: ProjectView;
}

function sameView(a: ProjectView, b: ProjectView): boolean {
  return (
    a.view === b.view &&
    a.plateView === b.plateView &&
    a.selectedColour === b.selectedColour &&
    a.bedPart === b.bedPart &&
    a.comparePaint === b.comparePaint
  );
}

/** What this tab should do about a project revision that another tab committed. */
export type RemoteChange =
  | { action: 'none' }
  | { action: 'conflict' }
  | { action: 'adopt'; record: SavedProject }
  | { action: 'reopen'; record: SavedProject };

/**
 * Another tab saved while this one held the project. Identical artwork bytes mean only the
 * view or the name changed, so the new revision can be adopted without reopening the
 * project, which would reset the screen and drop the undo history. Different bytes need the
 * artwork reloaded. A pending draft, or a project removed elsewhere, is a conflict.
 *
 * An adopted record keeps `view`, this tab's screen: taking the other tab's view would make
 * an unchanged project look unsaved here and save the old view straight back.
 */
export function remoteChange(
  saved: SavedProject,
  remote: SavedProject | undefined,
  pending: boolean,
  view: ProjectView
): RemoteChange {
  if (remote?.revision === saved.revision) return { action: 'none' };
  if (pending || !remote) return { action: 'conflict' };
  return remote.digest === saved.digest
    ? { action: 'adopt', record: { ...remote, view } }
    : { action: 'reopen', record: remote };
}

interface SaveLane {
  /** The write already running, if any. */
  readonly flight: () => Promise<boolean> | undefined;
  /** Whether the current draft still differs from the saved record. */
  readonly pending: () => boolean;
  /** Whether saving is refused for now, by an upgrade or a conflict. */
  readonly blocked: () => boolean;
  /** Starts one write and answers whether it committed. */
  readonly write: () => Promise<boolean>;
}

/**
 * Saves until nothing is pending, settling a write that was already in flight first:
 * callers are told the workspace is safe only once the bytes they asked about committed.
 */
export async function settleSaves(lane: SaveLane): Promise<boolean> {
  const started = lane.flight();
  if (started && !(await started)) return false;
  while (lane.pending() && !lane.blocked()) {
    if (!(await lane.write())) return false;
  }
  return !lane.pending();
}

export function suggestedTitle(file: File): string {
  return baseName(file.name).slice(0, TITLE_LIMIT);
}

export function workspaceProblem(error: unknown): string {
  if (error instanceof WorkspaceError) return words.errors[error.code];
  if (error instanceof ProjectSaveError) {
    const { problem } = error;
    switch (problem.kind) {
      case 'settings':
        return words.errors.saveSettings;
      case 'inactive-settings':
        return words.errors.saveInactiveSettings(text.project.modes[problem.mode]);
      case 'artwork':
        return words.errors.saveArtwork;
      case 'file':
        return words.errors.saveFile;
    }
  }
  if (error instanceof DOMException && error.name === 'QuotaExceededError')
    return words.errors.full;
  return words.errors.unavailable;
}

export class Workspace {
  readonly bench: Workbench;
  #store: WorkspaceStore | undefined;
  #connectFlight: Promise<WorkspaceStore | undefined> | undefined;
  #flight: Promise<boolean> | undefined;
  #reads = 0;
  #disposed = false;
  #requestedPersistence = false;
  #record = $state.raw<SavedProject>();
  // The revision loaded into the workbench. Saving its own draft does not change the target.
  #loadedRevision = $state<number | null>(null);
  #readyBytes = $state.raw<{ version: number; file: Blob }>();
  #savedVersion = $state(-1);
  #sampleVersion = $state(-1);
  #failedDraft = $state.raw<SaveDraft>();
  #writing = $state(false);
  #refreshWhenIdle = $state(false);
  #localTheme = $state.raw<{ choice: ThemeChoice; useSnapshot: boolean }>();
  id = $state('');
  title = $state('');
  starting = $state(true);
  initialising = $state(true);
  busy = $state(false);
  conflict = $state(false);
  upgrade = $state(false);
  storageProblem = $state('');
  saveProblem = $state('');
  message = $state('');
  snapshot = $state.raw<WorkspaceSnapshot>();

  readonly projects = $derived(
    [...(this.snapshot?.projects ?? [])].sort(
      (a, b) => b.editedAt - a.editedAt || a.title.localeCompare(b.title)
    )
  );
  readonly profiles = $derived(
    [...(this.snapshot?.profiles ?? [])].sort((a, b) => a.name.localeCompare(b.name))
  );
  readonly activeProfile = $derived(this.profiles.find(({ id }) => id === this.bench.profileId));
  readonly profileUpdate = $derived.by(() => {
    const profile = this.activeProfile;
    const { bedWidthMm, bedDepthMm } = this.bench.settings;
    return profile && (profile.widthMm !== bedWidthMm || profile.depthMm !== bedDepthMm)
      ? text.printer.updated(profile.name, bedWidthMm, bedDepthMm, profile.widthMm, profile.depthMm)
      : '';
  });
  readonly defaultProfile = $derived(
    this.profiles.find(({ id }) => id === this.snapshot?.preferences.defaultProfileId)
  );
  readonly remote = $derived(this.snapshot?.projects.find(({ id }) => id === this.id));
  readonly removable = $derived(this.conflict || this.upgrade ? undefined : this.#record);
  readonly replacementTarget: ReplacementTarget | undefined = $derived(
    this.id && !this.starting ? { id: this.id, revision: this.#loadedRevision } : undefined
  );
  readonly themeChoice = $derived(this.#localTheme?.choice ?? this.snapshot?.preferences.theme);
  readonly sample = $derived.by(
    () => this.id !== '' && this.#sampleVersion === this.bench.editVersion
  );
  readonly pending = $derived.by(() => {
    if (!this.id || this.sample) return false;
    const saved = this.#record;
    return (
      !saved ||
      this.#savedVersion !== this.bench.editVersion ||
      this.title !== saved.title ||
      !sameView(this.bench.projectView, saved.view)
    );
  });
  readonly status = $derived(
    this.upgrade
      ? 'upgrade'
      : this.conflict
        ? 'conflict'
        : this.sample
          ? 'sample'
          : this.saveProblem
            ? 'failed'
            : this.pending || this.#writing
              ? 'saving'
              : 'saved'
  );
  readonly downloadName = $derived(projectFileName(this.#downloadTitle()));
  readonly suggestedPreset: Preset = $derived(
    this.snapshot?.preferences.defaults.projectMode === 'freeform'
      ? 'freeform'
      : this.snapshot?.preferences.defaults.sizing === 'pixels'
        ? 'pixel'
        : 'tile'
  );

  get canOpen(): boolean {
    return !this.initialising && !this.busy && this.bench.canOpen;
  }

  checkCanOpen(): boolean {
    if (this.canOpen) return true;
    this.message = text.project.openBusy;
    return false;
  }

  constructor(bench: Workbench) {
    this.bench = bench;
    $effect(() => {
      const { version, title, view } = this.#saveDraft();
      const failed = this.#failedDraft;
      const blocked =
        this.busy || this.initialising || this.bench.opening || this.upgrade || this.conflict;
      if (!this.pending) {
        // Reverting to the committed state leaves no draft for a failure to describe, and
        // nothing for Retry to write. Keep a write's own failure, and a blocking error, visible.
        if (this.saveProblem && !blocked && !this.#writing) {
          this.saveProblem = '';
          this.#failedDraft = undefined;
        }
        return;
      }
      if (
        blocked ||
        this.#writing ||
        (failed?.version === version && failed.title === title && sameView(failed.view, view))
      )
        return;
      const timer = setTimeout(() => void this.flush(), SAVE_AFTER_MS);
      return () => {
        clearTimeout(timer);
      };
    });
    $effect(() => {
      if (this.#refreshWhenIdle && this.canOpen) void this.refresh();
    });
    $effect(() => {
      if (!this.pending) return;
      const warn = (event: BeforeUnloadEvent): void => {
        event.preventDefault();
      };
      window.addEventListener('beforeunload', warn);
      return () => {
        window.removeEventListener('beforeunload', warn);
      };
    });
  }

  async initialise(): Promise<void> {
    try {
      const store = await this.#connect();
      if (!store) return;
      this.snapshot = await store.snapshot();
      const selected = this.snapshot.projects.find(
        ({ id }) => id === this.snapshot?.preferences.lastProjectId
      );
      if (selected) await this.#openSaved(selected, false);
    } catch (error) {
      this.storageProblem = workspaceProblem(error);
    } finally {
      this.initialising = false;
      void this.refresh();
    }
  }

  dispose(): void {
    this.#disposed = true;
    this.#store?.close();
    this.#store = undefined;
  }

  async #connect(): Promise<WorkspaceStore | undefined> {
    if (this.upgrade || this.#disposed) return;
    if (this.#store) return this.#store;
    this.#connectFlight ??= openWorkspace(
      () => void this.refresh(),
      () => {
        this.upgrade = true;
        this.#store = undefined;
      },
      (closed) => {
        if (this.#store !== closed) return;
        this.#store = undefined;
        this.storageProblem = words.errors.unavailable;
      }
    )
      .then(
        (store) => {
          if (this.#disposed || this.upgrade) {
            store.close();
            return;
          }
          this.#store = store;
          this.storageProblem = '';
          return store;
        },
        (error: unknown) => {
          this.storageProblem = workspaceProblem(error);
          if (error instanceof WorkspaceError && error.code === 'upgrade') this.upgrade = true;
          return undefined;
        }
      )
      .finally(() => {
        this.#connectFlight = undefined;
      });
    return this.#connectFlight;
  }

  refresh = async (): Promise<void> => {
    const store = this.#store;
    if (!store || this.#disposed || this.initialising) return;
    if (this.canOpen) this.#refreshWhenIdle = false;
    const read = ++this.#reads;
    // Only a snapshot taken after the preference commit can replace the local choice.
    const theme = this.#localTheme;
    const useSnapshot = theme?.useSnapshot;
    try {
      const next = await store.snapshot();
      if (read !== this.#reads || store !== this.#store) return;
      this.snapshot = next;
      if (useSnapshot && this.#localTheme === theme) this.#localTheme = undefined;
      this.storageProblem = '';
      const saved = this.#record;
      if (!saved || this.#flight || this.conflict) return;
      const remote = next.projects.find(({ id }) => id === saved.id);
      const change = remoteChange(saved, remote, this.pending, this.bench.projectView);
      if (change.action === 'none') return;
      if (!this.canOpen) {
        // Reread once the export or project switch finishes. Either copy may have changed.
        this.#refreshWhenIdle = true;
        return;
      }
      if (change.action === 'conflict') this.conflict = true;
      else if (change.action === 'adopt') this.#adoptRemote(change.record);
      else await this.#transition(() => this.#openSaved(change.record, false), false);
    } catch (error) {
      if (store === this.#store) this.storageProblem = workspaceProblem(error);
    }
  };

  async flush(): Promise<boolean> {
    return settleSaves({
      flight: () => this.#flight,
      pending: () => this.pending,
      blocked: () => this.upgrade || this.conflict,
      write: () =>
        (this.#flight ??= this.#write().finally(() => {
          this.#flight = undefined;
          void this.refresh();
        }))
    });
  }

  /**
   * Takes another tab's view- or name-only save without reloading the artwork. The record
   * already carries this tab's view, so the project does not look unsaved afterwards.
   */
  #adoptRemote(record: SavedProject): void {
    this.#record = record;
    this.#loadedRevision = record.revision;
    this.title = record.title;
    this.bench.profileId = record.profileId;
  }

  #saveDraft(): SaveDraft {
    return { version: this.bench.editVersion, title: this.title, view: this.bench.projectView };
  }

  async #write(): Promise<boolean> {
    let draft = this.#saveDraft();
    this.#writing = true;
    this.saveProblem = '';
    try {
      const store = await this.#connect();
      if (!store) {
        this.saveProblem = this.storageProblem || words.errors.unavailable;
        return false;
      }
      draft = this.#saveDraft();
      const { version, title, view } = draft;
      const input = {
        id: this.id,
        title,
        view,
        mode: this.bench.settings.projectMode,
        profileId: this.bench.profileId
      };
      const expected = this.#record?.revision ?? null;
      const file =
        expected === null || this.#savedVersion !== version
          ? await this.#fileFor(version)
          : undefined;
      const saved = await store.saveProject({ ...input, ...(file ? { file } : {}) }, expected);
      this.#record = saved;
      this.#savedVersion = version;
      this.#readyBytes = undefined;
      if (this.bench.profileId === input.profileId) this.bench.profileId = saved.profileId;
      if (expected === null) {
        this.#requestPersistence();
        await this.#rememberLast(store, input.id);
      }
      return true;
    } catch (error) {
      if (error instanceof WorkspaceError && error.code === 'conflict') this.conflict = true;
      else this.saveProblem = workspaceProblem(error);
      return false;
    } finally {
      this.#failedDraft = this.saveProblem ? draft : undefined;
      this.#writing = false;
    }
  }

  async #fileFor(version: number): Promise<Blob> {
    if (this.#readyBytes?.version === version) return this.#readyBytes.file;
    if (this.#record && this.#savedVersion === version) return this.#record.file;
    try {
      return await this.bench.projectBytes(true);
    } catch (error) {
      if (error instanceof ProjectSaveError) throw error;
      throw new ProjectSaveError({ kind: 'file' }, { cause: error });
    }
  }

  async #rememberLast(store: WorkspaceStore, id: string): Promise<void> {
    try {
      await store.updatePreferences({ lastProjectId: id });
    } catch (error) {
      this.message = workspaceProblem(error);
    }
  }

  #requestPersistence(): void {
    if (this.#requestedPersistence) return;
    this.#requestedPersistence = true;
    // Not every browser offers persistent storage, or grants it when asked.
    const browser: { storage?: Partial<StorageManager> } = navigator;
    void browser.storage?.persist?.().catch(() => undefined);
  }

  async #transition(task: () => Promise<boolean>, refresh = true): Promise<boolean> {
    if (!this.checkCanOpen()) return false;
    this.busy = true;
    this.message = '';
    try {
      return await task();
    } catch (error) {
      this.message = workspaceProblem(error);
      return false;
    } finally {
      this.busy = false;
      if (refresh) void this.refresh();
    }
  }

  async #canLeave(): Promise<boolean> {
    if (this.#flight) await this.#flight;
    if (!(await this.flush()) && !window.confirm(words.leaveUnsaved(this.title))) return false;
    if (this.#record && !this.pending && !this.conflict)
      this.bench.rememberHistory(this.id, this.#record.revision);
    return true;
  }

  #defaults(): Settings {
    const defaults = this.snapshot
      ? normaliseDefaults(this.snapshot.preferences.defaults)
      : legacyDefaults();
    const profile = this.defaultProfile;
    return {
      ...defaults,
      ...(profile ? { bedWidthMm: profile.widthMm, bedDepthMm: profile.depthMm } : {})
    };
  }

  goNew = (): void => {
    if (this.busy || this.initialising) return;
    this.starting = true;
    this.message = '';
  };

  openSaved = (id: string): Promise<boolean> =>
    this.#transition(async () => {
      if (id === this.id) {
        this.starting = false;
        return true;
      }
      if (!(await this.#canLeave())) return false;
      const store = await this.#requireStore();
      const record = (await store.snapshot()).projects.find((project) => project.id === id);
      if (!record) {
        this.message = words.errors.conflict;
        return false;
      }
      return this.#openSaved(record, true);
    });

  async #openSaved(record: SavedProject, remember: boolean): Promise<boolean> {
    const starting = this.starting;
    const sameProject = record.id === this.id;
    const { id, revision, view, profileId } = record;
    if (
      !(await this.bench.open(new File([record.file], projectFileName(record.title)), {
        profileId,
        resume: { id, revision, view }
      }))
    )
      return false;
    this.#activate(id, record.title, record);
    if (sameProject && !remember) this.starting = starting;
    if (remember && this.#store) await this.#rememberLast(this.#store, id);
    return true;
  }

  #activate(id: string, title: string, record?: SavedProject): void {
    this.id = id;
    this.title = title;
    this.#record = record;
    this.#loadedRevision = record?.revision ?? null;
    this.#readyBytes = undefined;
    this.#savedVersion = record ? this.bench.editVersion : -1;
    this.#sampleVersion = -1;
    this.conflict = false;
    this.saveProblem = '';
    this.#failedDraft = undefined;
    this.starting = false;
  }

  startFile = (file: File, preset?: Preset): Promise<boolean> =>
    this.#transition(async () => {
      if (!(await this.#canLeave())) return false;
      const imported = file.name.toLowerCase().endsWith('.stencilator');
      if (
        !(await this.bench.open(file, {
          defaults: this.#defaults(),
          ...(preset ? { preset } : {}),
          profileId: imported ? null : (this.defaultProfile?.id ?? null)
        }))
      )
        return false;
      this.#activate(crypto.randomUUID(), suggestedTitle(file));
      if (imported) this.#readyBytes = { version: this.bench.editVersion, file };
      return true;
    });

  startText = (target?: ReplacementTarget): Promise<boolean> =>
    this.#transition(async () => {
      if (!(await (target ? this.#confirmReplacement(target) : this.#canLeave()))) return false;
      if (
        !(await this.bench.typeText({
          defaults: this.#defaults(),
          replace: target !== undefined,
          profileId: target ? this.bench.profileId : (this.defaultProfile?.id ?? null)
        }))
      )
        return false;
      if (!target) this.#activate(crypto.randomUUID(), words.textTitle);
      return true;
    });

  startSample = (preset: Preset): Promise<boolean> =>
    this.#transition(async () => {
      if (!(await this.#canLeave())) return false;
      if (
        !(await this.bench.openExample({
          defaults: this.#defaults(),
          preset,
          profileId: this.defaultProfile?.id ?? null
        }))
      )
        return false;
      this.#activate(crypto.randomUUID(), words.samples[preset]);
      this.#sampleVersion = this.bench.editVersion;
      return true;
    });

  canReplace(target: ReplacementTarget | undefined): boolean {
    const current = this.replacementTarget;
    return (
      target !== undefined && current?.id === target.id && current.revision === target.revision
    );
  }

  async #confirmReplacement(target: ReplacementTarget): Promise<boolean> {
    const current = (): boolean => this.canReplace(target);
    if (!current()) {
      this.message = words.replaceChanged;
      return false;
    }
    const saved = await this.flush();
    if (!current()) {
      this.message = words.replaceChanged;
      return false;
    }
    // A clean draft or a successful save needs no warning. A failed save leaves the draft
    // here, so ask before the replacement clears it.
    return saved || !this.pending || window.confirm(words.replaceUnsaved(this.title));
  }

  replace = (file: File, target: ReplacementTarget): Promise<boolean> =>
    this.#transition(async () => {
      if (!(await this.#confirmReplacement(target))) return false;
      return this.bench.open(file, { replace: true, profileId: this.bench.profileId });
    });

  keep = (): void => {
    this.#sampleVersion = -1;
    void this.flush();
  };

  rename = (name: string): void => {
    const title = name.trim().slice(0, TITLE_LIMIT);
    if (!title || title === this.title) return;
    this.title = title;
    this.keep();
  };

  download = (): Promise<void> => {
    if (!this.id || this.busy) return Promise.resolve();
    const cached =
      this.#savedVersion === this.bench.editVersion
        ? this.#record?.file
        : this.#readyBytes?.version === this.bench.editVersion
          ? this.#readyBytes.file
          : undefined;
    // Separators in a project name do not have to be separators in a download name.
    return this.bench.saveProject(this.#downloadTitle(), cached);
  };

  #downloadTitle(): string {
    return this.title.replace(/[\\/]/g, '_');
  }

  loadRemote = (): Promise<boolean> =>
    this.#transition(async () => {
      if (this.#flight) await this.#flight;
      const store = await this.#requireStore();
      const record = (await store.snapshot()).projects.find(({ id }) => id === this.id);
      if (!record) {
        this.message = words.errors.conflict;
        return false;
      }
      this.bench.forgetHistory(this.id);
      return this.#openSaved(record, true);
    });

  copy = (recovery = false): Promise<boolean> =>
    this.#transition(async () => {
      if (this.#flight) await this.#flight;
      if (!recovery && !(await this.#canLeave())) return false;
      const store = await this.#connect();
      if (!store) {
        this.message = this.storageProblem || words.errors.unavailable;
        return false;
      }
      const id = crypto.randomUUID();
      const title = this.#copyTitle();
      const record = await store.saveProject(
        {
          id,
          title,
          mode: this.bench.settings.projectMode,
          view: this.bench.projectView,
          profileId: this.bench.profileId,
          file: await this.#fileFor(this.bench.editVersion)
        },
        null
      );
      this.#activate(id, title, record);
      this.bench.profileId = record.profileId;
      this.bench.resetHistory();
      this.#requestPersistence();
      await this.#rememberLast(store, id);
      return true;
    });

  #copyTitle(): string {
    const names = new SvelteSet(this.projects.map(({ title }) => title.toLocaleLowerCase()));
    for (let number = 1; ; number++) {
      const suffix = words.copySuffix(number);
      const title = `${this.title.slice(0, TITLE_LIMIT - suffix.length)}${suffix}`;
      if (!names.has(title.toLocaleLowerCase())) return title;
    }
  }

  remove = (confirmed: SavedProject): Promise<boolean> =>
    this.#transition(async () => {
      if (this.#flight) await this.#flight;
      if (!this.#store || this.removable !== confirmed) throw new WorkspaceError('conflict');
      await this.#store.removeProject(confirmed.id, confirmed.revision);
      this.bench.forgetHistory(confirmed.id);
      this.#activate('', '');
      this.starting = true;
      return true;
    });

  async #requireStore(): Promise<WorkspaceStore> {
    const store = await this.#connect();
    if (store) return store;
    if (this.upgrade) throw new WorkspaceError('upgrade');
    throw new DOMException(words.errors.unavailable, 'InvalidStateError');
  }

  async saveProfile(profile: ProfileWrite, previous?: PrinterProfile): Promise<PrinterProfile> {
    const store = await this.#requireStore();
    const saved = await store.saveProfile(profile, previous?.revision ?? null);
    await this.refresh();
    return saved;
  }

  async deleteProfile(profile: PrinterProfile): Promise<void> {
    // Finish a queued project write before the removal clears its profile link.
    if (this.bench.profileId === profile.id && !(await this.flush()))
      throw new WorkspaceError('conflict');
    const store = await this.#requireStore();
    await store.removeProfile(profile.id, profile.revision);
    await this.refresh();
  }

  async setDefaultProfile(id: string | null): Promise<void> {
    const store = await this.#requireStore();
    await store.updatePreferences({ defaultProfileId: id });
    await this.refresh();
  }

  async setTheme(theme: ThemeChoice): Promise<void> {
    const local = { choice: theme, useSnapshot: false };
    this.#localTheme = local;
    this.message = '';
    try {
      const store = await this.#requireStore();
      await store.updatePreferences({ theme });
      local.useSnapshot = true;
      await this.refresh();
    } catch (error) {
      if (this.#localTheme === local) this.message = workspaceProblem(error);
    }
  }

  async planBackup(): Promise<BackupPlan> {
    const store = await this.#requireStore();
    return planBackups(await store.snapshot());
  }

  readBackup = (file: Blob): Promise<WorkspaceBackup> =>
    decodeBackup(file, this.bench.validateProject);

  async previewBackup(backup: WorkspaceBackup): Promise<ReturnType<typeof previewRestore>> {
    const store = await this.#requireStore();
    return previewRestore(backup, await store.snapshot());
  }

  async restoreBackup(
    plan: ReturnType<typeof previewRestore>,
    preferences: boolean
  ): Promise<boolean> {
    const theme = this.#localTheme;
    const store = await this.#requireStore();
    const imported = await store.restore(plan, preferences);
    if (imported) {
      if (plan.projects.some(({ action }) => action !== 'skip')) this.#requestPersistence();
      if (preferences && theme) theme.useSnapshot = true;
      await this.refresh();
    }
    return imported;
  }
}
