/**
 * The browser's saved workspace: projects, printer profiles and preferences in one
 * IndexedDB database, with an optimistic revision fence so a stale tab cannot replace a
 * newer save or recreate a removed record. Nothing here talks to the engine or the
 * interface; it only reads and writes structured records.
 *
 * Every backup-relevant change advances a single workspace revision. A record's own
 * `revision` field is the workspace revision at the time it was last written, and is the
 * fence callers pass back to `saveProject`, `removeProject`, `saveProfile` and
 * `removeProfile`. `lastProjectId` is session state, not part of a backup, so it alone
 * does not advance the revision.
 */
import type { ProjectMode } from '../engine/client';
import type { Settings } from './settings';
import { sanitise } from './settings';
import { readStored } from './storage';

const DB_NAME = 'stencilator-workspace';
const DB_VERSION = 1;

/** The old local-storage keys the app wrote before the workspace database existed. */
const LEGACY_SETTINGS_KEY = 'stencilator.settings';
const LEGACY_THEME_KEY = 'stencilator.theme';

/** The shared channel other tabs listen on for committed changes. */
const CHANNEL = 'stencilator-workspace';

/** A place the interface keeps open: what it is showing and how. */
export interface ProjectView {
  view: 'picture' | 'chart' | 'both' | 'stencil' | 'result' | 'bed';
  plateView: 'top' | '3d';
  selectedColour: string | null;
  bedPart: 'frame' | null;
  comparePaint: boolean;
}

/** A record's original lineage and the specific saved entry it was restored from. */
export interface Origin {
  id: string;
  /** The incoming backup's record id, absent on older restores. */
  sourceId?: string;
  name: string;
  importedAs: string;
}

/** A saved project, with its `.stencilator` bytes and a fingerprint of them. */
export interface SavedProject {
  id: string;
  revision: number;
  editedAt: number;
  title: string;
  mode: ProjectMode;
  view: ProjectView;
  profileId: string | null;
  file: Blob;
  digest: string;
  origin?: Origin;
}

/**
 * What the interface gives the store to save. `file` keeps the current bytes when
 * omitted (the store only hashes new bytes); a brand-new project must provide it.
 */
type ProjectWrite = Omit<SavedProject, 'revision' | 'editedAt' | 'digest' | 'origin' | 'file'> & {
  file?: Blob;
};

/** A saved printer profile. The optional slicer sizes are references only. */
export interface PrinterProfile {
  id: string;
  revision: number;
  name: string;
  widthMm: number;
  depthMm: number;
  lineWidthMm?: number;
  layerHeightMm?: number;
  nozzleMm?: number;
  origin?: Origin;
}

/** What the interface gives the store to save a profile. */
export type ProfileWrite = Omit<PrinterProfile, 'revision' | 'origin'>;

/** App preferences kept with the workspace. */
export interface WorkspacePreferences {
  defaults: Settings;
  theme: 'auto' | 'light' | 'dark';
  defaultProfileId: string | null;
  lastProjectId: string | null;
}

/** Everything in the workspace, as one consistent read. */
export interface WorkspaceSnapshot {
  revision: number;
  projects: SavedProject[];
  profiles: PrinterProfile[];
  preferences: WorkspacePreferences;
}

/** One record of a restore preview: what to do with a source record. */
export interface RestoreEntry<T> {
  source: T;
  /** The actual resulting record; for `skip` this points at the existing record. */
  record: T;
  action: 'add' | 'skip' | 'copy';
}

/** A prepared restore: the exact records to add or copy, and the revision it previewed. */
export interface RestorePlan {
  revision: number;
  projects: RestoreEntry<SavedProject>[];
  profiles: RestoreEntry<PrinterProfile>[];
  preferences: Omit<WorkspacePreferences, 'lastProjectId'>;
}

export type WorkspaceErrorCode =
  'conflict' | 'upgrade' | 'invalid' | 'unsupported' | 'too-large' | 'validation-unavailable';

/** A typed failure the interface can show a message for. */
export class WorkspaceError extends Error {
  readonly code: WorkspaceErrorCode;

  constructor(code: WorkspaceErrorCode, message?: string) {
    super(message ?? code);
    this.name = 'WorkspaceError';
    this.code = code;
  }
}

/** The engine's accepted print-bed sides, shared with the fit check. */
const BED_MIN_MM = 50;
const BED_MAX_MM = 2000;

function defaultPreferences(): WorkspacePreferences {
  return {
    defaults: sanitise(undefined),
    theme: 'auto',
    defaultProfileId: null,
    lastProjectId: null
  };
}

/** Preferences migrated from the old local storage, exactly once, on first open. */
function migratedPreferences(): WorkspacePreferences {
  const theme = readStored(LEGACY_THEME_KEY);
  return {
    defaults: sanitise(readStored(LEGACY_SETTINGS_KEY)),
    theme: theme === 'light' || theme === 'dark' || theme === 'auto' ? theme : 'auto',
    defaultProfileId: null,
    lastProjectId: null
  };
}

/** The SHA-256 fingerprint of a file, as lowercase hex. */
async function digest(file: Blob): Promise<string> {
  const hash = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
  return Array.from(new Uint8Array(hash), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

/** Resolves when a single request succeeds, and rejects with its error otherwise. */
function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => {
      resolve(request.result);
    };
    request.onerror = () => {
      reject(request.error ?? new Error('IndexedDB request failed'));
    };
  });
}

/** The value of a workspace store record, or `undefined` when it has not been set. */
async function workspaceValue<T>(store: IDBObjectStore, key: string): Promise<T | undefined> {
  const record = await requestResult<unknown>(store.get(key));
  return (record as { value?: T } | undefined)?.value;
}

function putWorkspace(store: IDBObjectStore, key: string, value: unknown): void {
  store.put({ key, value });
}

function transactionDone(tx: IDBTransaction): Promise<void> {
  const done = new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => {
      resolve();
    };
    // A request error can bubble before IndexedDB sets the transaction's final error.
    tx.onabort = () => {
      reject(tx.error ?? new DOMException('The transaction was aborted.', 'AbortError'));
    };
  });
  // A request can reject first, leaving the caller no chance to await completion.
  void done.catch(() => undefined);
  return done;
}

/**
 * Runs `work` inside a readwrite transaction and waits for it to commit. A throw aborts
 * the transaction and leaves nothing written; the rejection is handled without leaving an
 * unhandled promise behind.
 */
async function transact<T>(
  db: IDBDatabase,
  stores: string[],
  work: (tx: IDBTransaction) => Promise<T>
): Promise<T> {
  const tx = db.transaction(stores, 'readwrite');
  const done = transactionDone(tx);
  try {
    const result = await work(tx);
    await done;
    return result;
  } catch (error) {
    try {
      tx.abort();
    } catch {
      // Already finished or aborted.
    }
    throw error;
  }
}

/** A finite timestamp in the past, for records written now. */
function now(): number {
  return Date.now();
}

/** Whether a name is a usable, non-empty string. */
const isName = (value: unknown): value is string =>
  typeof value === 'string' && value.trim().length > 0;

export interface WorkspaceStore {
  snapshot(): Promise<WorkspaceSnapshot>;
  saveProject(input: ProjectWrite, expectedRevision: number | null): Promise<SavedProject>;
  removeProject(id: string, expectedRevision: number): Promise<void>;
  saveProfile(input: ProfileWrite, expectedRevision: number | null): Promise<PrinterProfile>;
  removeProfile(id: string, expectedRevision: number): Promise<void>;
  updatePreferences(patch: Partial<WorkspacePreferences>): Promise<void>;
  /** Returns false for a stale preview, true even when the valid preview needs no writes. */
  restore(plan: RestorePlan, restorePreferences: boolean): Promise<boolean>;
  close(): void;
}

class Store implements WorkspaceStore {
  readonly #db: IDBDatabase;
  readonly #channel: BroadcastChannel;
  #closed = false;

  constructor(
    db: IDBDatabase,
    onChange: () => void,
    onVersionChange: () => void,
    onClose: (store: WorkspaceStore) => void
  ) {
    this.#db = db;
    this.#channel = new BroadcastChannel(CHANNEL);
    this.#channel.onmessage = onChange;
    db.onversionchange = () => {
      this.close();
      onVersionChange();
    };
    db.onclose = () => {
      this.close();
      onClose(this);
    };
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#channel.close();
    this.#db.onversionchange = null;
    this.#db.onclose = null;
    this.#db.close();
  }

  #broadcast(): void {
    if (this.#closed) return;
    this.#channel.postMessage('changed');
  }

  async snapshot(): Promise<WorkspaceSnapshot> {
    const tx = this.#db.transaction(['projects', 'profiles', 'workspace'], 'readonly');
    const done = transactionDone(tx);
    const projects = await requestResult(
      tx.objectStore('projects').getAll() as IDBRequest<SavedProject[]>
    );
    const profiles = await requestResult(
      tx.objectStore('profiles').getAll() as IDBRequest<PrinterProfile[]>
    );
    const workspace = tx.objectStore('workspace');
    const revision = (await workspaceValue<number>(workspace, 'revision')) ?? 0;
    const preferences =
      (await workspaceValue<WorkspacePreferences>(workspace, 'preferences')) ??
      defaultPreferences();
    await done;
    return { revision, projects, profiles, preferences };
  }

  async saveProject(input: ProjectWrite, expectedRevision: number | null): Promise<SavedProject> {
    // Hash a new file's bytes before the transaction: a readwrite transaction that awaits
    // a Blob read or Web Crypto would auto-commit before the following write.
    const providedFile = input.file;
    const providedDigest = providedFile ? await digest(providedFile) : undefined;
    const saved = await transact(this.#db, ['projects', 'profiles', 'workspace'], async (tx) => {
      const projectStore = tx.objectStore('projects');
      const existing = await requestResult(
        projectStore.get(input.id) as IDBRequest<SavedProject | undefined>
      );
      this.#fence(existing, expectedRevision, input.id);

      let file: Blob;
      let digestText: string;
      let editedAt: number;
      if (existing) {
        if (providedFile !== undefined && providedDigest !== undefined) {
          file = providedFile;
          digestText = providedDigest;
          editedAt =
            providedDigest !== existing.digest ||
            input.title !== existing.title ||
            input.profileId !== existing.profileId
              ? now()
              : existing.editedAt;
        } else {
          file = existing.file;
          digestText = existing.digest;
          editedAt =
            input.title !== existing.title || input.profileId !== existing.profileId
              ? now()
              : existing.editedAt;
        }
      } else {
        if (providedFile === undefined || providedDigest === undefined) {
          throw new WorkspaceError('invalid', 'A new project needs its file bytes.');
        }
        file = providedFile;
        digestText = providedDigest;
        editedAt = now();
      }

      // Never persist a link to a profile that is no longer there.
      const profileId = input.profileId ? await this.#linkedProfile(tx, input.profileId) : null;

      const workspace = tx.objectStore('workspace');
      const revision = ((await workspaceValue<number>(workspace, 'revision')) ?? 0) + 1;
      const record: SavedProject = {
        id: input.id,
        revision,
        editedAt,
        title: input.title,
        mode: input.mode,
        view: input.view,
        profileId,
        file,
        digest: digestText,
        ...(existing?.origin ? { origin: existing.origin } : {})
      };
      projectStore.put(record);
      putWorkspace(workspace, 'revision', revision);
      return record;
    });
    this.#broadcast();
    return saved;
  }

  async removeProject(id: string, expectedRevision: number): Promise<void> {
    await transact(this.#db, ['projects', 'workspace'], async (tx) => {
      const projectStore = tx.objectStore('projects');
      const existing = await requestResult(
        projectStore.get(id) as IDBRequest<SavedProject | undefined>
      );
      if (existing?.revision !== expectedRevision) throw new WorkspaceError('conflict');
      projectStore.delete(id);
      const workspace = tx.objectStore('workspace');
      const preferences =
        (await workspaceValue<WorkspacePreferences>(workspace, 'preferences')) ??
        defaultPreferences();
      if (preferences.lastProjectId === id) {
        putWorkspace(workspace, 'preferences', { ...preferences, lastProjectId: null });
      }
      const revision = ((await workspaceValue<number>(workspace, 'revision')) ?? 0) + 1;
      putWorkspace(workspace, 'revision', revision);
    });
    this.#broadcast();
  }

  async saveProfile(input: ProfileWrite, expectedRevision: number | null): Promise<PrinterProfile> {
    const saved = await transact(this.#db, ['profiles', 'workspace'], async (tx) => {
      this.#checkProfile(input);
      const profileStore = tx.objectStore('profiles');
      const existing = await requestResult(
        profileStore.get(input.id) as IDBRequest<PrinterProfile | undefined>
      );
      this.#fence(existing, expectedRevision, input.id);
      const workspace = tx.objectStore('workspace');
      const revision = ((await workspaceValue<number>(workspace, 'revision')) ?? 0) + 1;
      const record: PrinterProfile = {
        id: input.id,
        revision,
        name: input.name,
        widthMm: input.widthMm,
        depthMm: input.depthMm,
        ...(input.lineWidthMm !== undefined ? { lineWidthMm: input.lineWidthMm } : {}),
        ...(input.layerHeightMm !== undefined ? { layerHeightMm: input.layerHeightMm } : {}),
        ...(input.nozzleMm !== undefined ? { nozzleMm: input.nozzleMm } : {}),
        ...(existing?.origin ? { origin: existing.origin } : {})
      };
      profileStore.put(record);
      putWorkspace(workspace, 'revision', revision);
      return record;
    });
    this.#broadcast();
    return saved;
  }

  async removeProfile(id: string, expectedRevision: number): Promise<void> {
    await transact(this.#db, ['profiles', 'projects', 'workspace'], async (tx) => {
      const profileStore = tx.objectStore('profiles');
      const existing = await requestResult(
        profileStore.get(id) as IDBRequest<PrinterProfile | undefined>
      );
      if (existing?.revision !== expectedRevision) throw new WorkspaceError('conflict');
      const workspace = tx.objectStore('workspace');
      const revision = ((await workspaceValue<number>(workspace, 'revision')) ?? 0) + 1;
      profileStore.delete(id);
      // Detach the link and re-fence every project that used it; their applied bed stays
      // inside their saved bytes.
      const projectStore = tx.objectStore('projects');
      const projects = await requestResult(projectStore.getAll() as IDBRequest<SavedProject[]>);
      for (const project of projects) {
        if (project.profileId === id) projectStore.put({ ...project, profileId: null, revision });
      }
      const preferences =
        (await workspaceValue<WorkspacePreferences>(workspace, 'preferences')) ??
        defaultPreferences();
      if (preferences.defaultProfileId === id) {
        putWorkspace(workspace, 'preferences', { ...preferences, defaultProfileId: null });
      }
      putWorkspace(workspace, 'revision', revision);
    });
    this.#broadcast();
  }

  async updatePreferences(patch: Partial<WorkspacePreferences>): Promise<void> {
    // A default profile must still exist: read it in the same transaction as the write,
    // so a concurrent deletion cannot leave a dangling preference behind.
    const stores = patch.defaultProfileId ? ['profiles', 'workspace'] : ['workspace'];
    await transact(this.#db, stores, async (tx) => {
      if (patch.defaultProfileId) {
        const profile = await requestResult(
          tx.objectStore('profiles').get(patch.defaultProfileId) as IDBRequest<
            PrinterProfile | undefined
          >
        );
        if (!profile) throw new WorkspaceError('conflict');
      }
      const workspace = tx.objectStore('workspace');
      const preferences =
        (await workspaceValue<WorkspacePreferences>(workspace, 'preferences')) ??
        defaultPreferences();
      putWorkspace(workspace, 'preferences', { ...preferences, ...patch });
      // `lastProjectId` is session state; the rest belongs in a backup.
      if ('defaults' in patch || 'theme' in patch || 'defaultProfileId' in patch) {
        const revision = ((await workspaceValue<number>(workspace, 'revision')) ?? 0) + 1;
        putWorkspace(workspace, 'revision', revision);
      }
    });
    this.#broadcast();
  }

  async restore(plan: RestorePlan, restorePreferences: boolean): Promise<boolean> {
    const projects = plan.projects.filter(({ action }) => action !== 'skip');
    const profiles = plan.profiles.filter(({ action }) => action !== 'skip');
    const hasChanges = restorePreferences || projects.length > 0 || profiles.length > 0;
    const applied = await transact(this.#db, ['projects', 'profiles', 'workspace'], async (tx) => {
      const workspace = tx.objectStore('workspace');
      const revision = (await workspaceValue<number>(workspace, 'revision')) ?? 0;
      if (revision !== plan.revision) return false;
      if (!hasChanges) return true;
      const next = revision + 1;
      const projectStore = tx.objectStore('projects');
      for (const { record } of projects) {
        projectStore.put({ ...record, revision: next });
      }
      const profileStore = tx.objectStore('profiles');
      for (const { record } of profiles) {
        profileStore.put({ ...record, revision: next });
      }
      if (restorePreferences) {
        const current =
          (await workspaceValue<WorkspacePreferences>(workspace, 'preferences')) ??
          defaultPreferences();
        putWorkspace(workspace, 'preferences', {
          ...plan.preferences,
          lastProjectId: current.lastProjectId
        });
      }
      putWorkspace(workspace, 'revision', next);
      return true;
    });
    if (applied && hasChanges) this.#broadcast();
    return applied;
  }

  #fence(
    existing: { revision: number } | undefined,
    expectedRevision: number | null,
    id: string
  ): void {
    if (expectedRevision === null) {
      if (existing) throw new WorkspaceError('conflict', `A project ${id} already exists.`);
    } else if (existing?.revision !== expectedRevision) {
      throw new WorkspaceError('conflict');
    }
  }

  #checkProfile(profile: ProfileWrite): void {
    if (!isName(profile.name))
      throw new WorkspaceError('invalid', 'A printer profile needs a name.');
    this.#checkBed(profile.widthMm);
    this.#checkBed(profile.depthMm);
    for (const value of [profile.lineWidthMm, profile.layerHeightMm, profile.nozzleMm]) {
      if (value !== undefined && (!Number.isFinite(value) || value <= 0)) {
        throw new WorkspaceError('invalid', 'The optional slicer sizes must be positive.');
      }
    }
  }

  #checkBed(mm: number): void {
    if (!Number.isFinite(mm) || mm < BED_MIN_MM || mm > BED_MAX_MM) {
      throw new WorkspaceError('invalid', 'The bed sides must be between 50 and 2000 mm.');
    }
  }

  /** The profile's id if it still exists, else `null`, so a missing link is never written. */
  async #linkedProfile(tx: IDBTransaction, id: string): Promise<string | null> {
    const profile = await requestResult(
      tx.objectStore('profiles').get(id) as IDBRequest<PrinterProfile | undefined>
    );
    return profile ? id : null;
  }
}

/**
 * Opens the workspace database, migrating the old local-storage preferences once on first
 * use. `onChange` runs when another tab commits a change; `onVersionChange` runs when
 * another tab wants a newer schema. `onClose` runs if the connection closes unexpectedly.
 *
 * Quota and disabled-storage exceptions propagate to the caller.
 */
export function openWorkspace(
  onChange: () => void,
  onVersionChange: () => void,
  onClose: (store: WorkspaceStore) => void
): Promise<WorkspaceStore> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      db.createObjectStore('projects', { keyPath: 'id' });
      db.createObjectStore('profiles', { keyPath: 'id' });
      const workspace = db.createObjectStore('workspace', { keyPath: 'key' });
      workspace.put({ key: 'preferences', value: migratedPreferences() });
      workspace.put({ key: 'revision', value: 0 });
    };
    request.onblocked = () => {
      reject(new WorkspaceError('upgrade', 'Another tab is blocking the workspace upgrade.'));
    };
    request.onsuccess = () => {
      const db = request.result;
      if (db.version > DB_VERSION) {
        db.close();
        reject(new WorkspaceError('upgrade', 'The workspace uses a newer storage version.'));
        return;
      }
      resolve(new Store(db, onChange, onVersionChange, onClose));
    };
    request.onerror = () => {
      const error = request.error;
      // Opening with an older version than the database already has fails with a
      // VersionError rather than opening the newer schema; offer the same reload as a
      // tab that received a versionchange.
      if (error instanceof DOMException && error.name === 'VersionError') {
        reject(new WorkspaceError('upgrade', 'The workspace uses a newer storage version.'));
        return;
      }
      reject(error ?? new Error('The workspace could not be opened.'));
    };
  });
}
