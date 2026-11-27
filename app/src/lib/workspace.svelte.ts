/** Keeps the live workbench and its saved browser record in step. */
import { SvelteSet } from 'svelte/reactivity';
import { ProjectSaveError } from '../engine/client';
import { baseName, projectFileName } from './names';
import type { Settings } from './settings';
import { sanitise } from './settings';
import { readStored } from './storage';
import { text } from './text';
import type { ThemeChoice } from './theme.svelte';
import { type Preset, type Workbench } from './workbench.svelte';
import {
  decodeBackup,
  planBackups,
  previewRestore,
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
const legacyDefaults = (): Settings => sanitise(readStored('stencilator.settings'));

function sameView(a: ProjectView, b: ProjectView): boolean {
  return (
    a.view === b.view &&
    a.plateView === b.plateView &&
    a.selectedColour === b.selectedColour &&
    a.bedPart === b.bedPart &&
    a.comparePaint === b.comparePaint
  );
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
  #readyBytes = $state.raw<{ version: number; file: Blob }>();
  #savedVersion = $state(-1);
  #sampleVersion = $state(-1);
  #failedVersion = $state(-1);
  #writing = $state(false);
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

  constructor(bench: Workbench) {
    this.bench = bench;
    $effect(() => {
      const version = this.bench.editVersion;
      if (
        !this.pending ||
        this.busy ||
        this.initialising ||
        this.bench.opening ||
        this.upgrade ||
        this.conflict ||
        (this.saveProblem && this.#failedVersion === version)
      )
        return;
      const timer = setTimeout(() => void this.flush(), SAVE_AFTER_MS);
      return () => {
        clearTimeout(timer);
      };
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
  }

  async #connect(): Promise<WorkspaceStore | undefined> {
    if (this.upgrade || this.#disposed) return;
    if (this.#store) return this.#store;
    this.#connectFlight ??= openWorkspace(
      () => void this.refresh(),
      () => {
        this.upgrade = true;
        this.#store = undefined;
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
    const read = ++this.#reads;
    try {
      const next = await store.snapshot();
      if (read !== this.#reads || store !== this.#store) return;
      this.snapshot = next;
      this.storageProblem = '';
      const saved = this.#record;
      if (!saved || this.busy || this.#flight || this.conflict) return;
      const remote = next.projects.find(({ id }) => id === saved.id);
      if (remote?.revision === saved.revision) return;
      if (this.pending || !remote) this.conflict = true;
      else if (remote.digest === saved.digest && sameView(remote.view, saved.view)) {
        this.#record = remote;
        this.title = remote.title;
        this.bench.profileId = remote.profileId;
      } else await this.#transition(() => this.#openSaved(remote, false), false);
    } catch (error) {
      if (store === this.#store) this.storageProblem = workspaceProblem(error);
    }
  };

  async flush(): Promise<boolean> {
    while (this.pending && !this.upgrade && !this.conflict) {
      this.#flight ??= this.#write().finally(() => {
        this.#flight = undefined;
        void this.refresh();
      });
      if (!(await this.#flight)) return false;
    }
    return !this.pending;
  }

  async #write(): Promise<boolean> {
    let version = this.bench.editVersion;
    this.#writing = true;
    this.saveProblem = '';
    try {
      const store = await this.#connect();
      if (!store) {
        this.saveProblem = this.storageProblem || words.errors.unavailable;
        return false;
      }
      version = this.bench.editVersion;
      const input = {
        id: this.id,
        title: this.title,
        view: this.bench.projectView,
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
      this.#failedVersion = version;
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
    if (this.busy || this.initialising || !this.bench.canOpen) return false;
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
    const defaults = this.snapshot?.preferences.defaults ?? legacyDefaults();
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
      const record = (await this.#store?.snapshot())?.projects.find((project) => project.id === id);
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
    this.#readyBytes = undefined;
    this.#savedVersion = record ? this.bench.editVersion : -1;
    this.#sampleVersion = -1;
    this.conflict = false;
    this.saveProblem = '';
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

  startText = (replace = false): Promise<boolean> =>
    this.#transition(async () => {
      if (!replace && !(await this.#canLeave())) return false;
      if (replace && this.#flight) await this.#flight;
      if (
        !(await this.bench.typeText({
          defaults: this.#defaults(),
          replace,
          profileId: replace ? this.bench.profileId : (this.defaultProfile?.id ?? null)
        }))
      )
        return false;
      if (!replace) this.#activate(crypto.randomUUID(), words.textTitle);
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

  replace = (file: File): Promise<boolean> =>
    this.#transition(async () => {
      if (this.#flight) await this.#flight;
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
      const record = (await this.#store?.snapshot())?.projects.find(({ id }) => id === this.id);
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
    try {
      const store = await this.#connect();
      if (!store) return;
      await store.updatePreferences({ theme });
      await this.refresh();
    } catch (error) {
      this.message = workspaceProblem(error);
    }
  }

  async backupParts(): Promise<WorkspaceSnapshot[]> {
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
    const store = await this.#requireStore();
    const imported = await store.restore(plan, preferences);
    if (imported) {
      if (plan.projects.some(({ action }) => action !== 'skip')) this.#requestPersistence();
      await this.refresh();
    }
    return imported;
  }
}
