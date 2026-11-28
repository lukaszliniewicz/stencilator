import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { PrinterProfile, ProjectView, RestorePlan, WorkspaceStore } from './workspace-store';
import { openWorkspace } from './workspace-store';

/**
 * A small IndexedDB stand-in for the unit tests, which run in Node. It covers the API the
 * workspace store uses and keeps IndexedDB's rule that a transaction commits only when its
 * requests have settled, so a read and a write issued in one transaction stay together.
 * It is kept here so the Node suite needs neither a browser nor another dependency.
 */
type Handler = (() => void) | null;

interface FakeStoreOptions {
  keyPath: string;
}

class FakeRequest {
  onsuccess: Handler = null;
  onerror: Handler = null;
  onupgradeneeded: Handler = null;
  onblocked: Handler = null;
  result: unknown;
  error: unknown = null;
}

class FakeObjectStore {
  constructor(
    readonly transaction: FakeTransaction,
    readonly data: Map<string, unknown>,
    readonly keyPath: string
  ) {}

  get(key: unknown): FakeRequest {
    return this.transaction.request(() => this.data.get(String(key)));
  }

  getAll(): FakeRequest {
    return this.transaction.request(() => [...this.data.values()]);
  }

  put(record: unknown): FakeRequest {
    return this.transaction.request(() => {
      if (typeof record !== 'object' || record === null)
        throw new Error('A record must be an object.');
      this.data.set(String((record as Record<string, unknown>)[this.keyPath]), record);
    });
  }

  delete(key: unknown): FakeRequest {
    return this.transaction.request(() => {
      this.data.delete(String(key));
    });
  }
}

class FakeTransaction {
  oncomplete: Handler = null;
  onabort: Handler = null;
  onerror: Handler = null;
  error: unknown = null;
  readonly #before: Map<string, Map<string, unknown>>;
  #pending = 0;
  #finished = false;
  #timer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    readonly db: FakeDatabase,
    names: string[]
  ) {
    this.#before = new Map(names.map((name) => [name, new Map(db.store(name))]));
  }

  objectStore(name: string): FakeObjectStore {
    return new FakeObjectStore(this, this.db.store(name), this.db.keyPaths.get(name) ?? 'id');
  }

  /** Runs one operation, like a request: the handler must still be attached first. */
  request(work: () => unknown): FakeRequest {
    const request = new FakeRequest();
    this.#pending++;
    queueMicrotask(() => {
      if (this.#finished) return;
      try {
        request.result = structuredClone(work());
        request.onsuccess?.();
      } catch (error) {
        request.error = error;
        request.onerror?.();
      } finally {
        this.#pending--;
        this.#schedule();
      }
    });
    return request;
  }

  abort(): void {
    if (this.#finished) return;
    for (const [name, data] of this.#before) this.db.stores.set(name, data);
    this.#settle();
    this.onabort?.();
  }

  #schedule(): void {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => {
      if (this.#pending === 0 && !this.#finished) {
        this.#settle();
        this.oncomplete?.();
      }
    }, 0);
  }

  #settle(): void {
    this.#finished = true;
    clearTimeout(this.#timer);
  }
}

class FakeDatabase {
  readonly stores = new Map<string, Map<string, unknown>>();
  readonly keyPaths = new Map<string, string>();
  onversionchange: Handler = null;
  onclose: Handler = null;
  version = 0;

  createObjectStore(name: string, options: FakeStoreOptions): { put: (record: unknown) => void } {
    const data = new Map<string, unknown>();
    this.stores.set(name, data);
    this.keyPaths.set(name, options.keyPath);
    // The versionchange transaction commits on its own; writes apply at once.
    return {
      put: (record: unknown) => {
        if (typeof record !== 'object' || record === null)
          throw new Error('A record must be an object.');
        data.set(String((record as Record<string, unknown>)[options.keyPath]), record);
      }
    };
  }

  store(name: string): Map<string, unknown> {
    const data = this.stores.get(name) ?? new Map<string, unknown>();
    this.stores.set(name, data);
    return data;
  }

  transaction(names: string[]): FakeTransaction {
    return new FakeTransaction(this, names);
  }

  close(): void {
    this.onclose?.();
  }
}

/** Each call opens a fresh, empty workspace database. */
function fakeIndexedDb(): IDBFactory {
  const databases = new Map<string, FakeDatabase>();
  return {
    open: (name: string, version: number) => {
      const request = new FakeRequest();
      queueMicrotask(() => {
        let db = databases.get(name);
        const fresh = db === undefined;
        if (!db) {
          db = new FakeDatabase();
          databases.set(name, db);
        }
        request.result = db;
        if (fresh || db.version < version) {
          db.version = version;
          request.onupgradeneeded?.();
        }
        request.onsuccess?.();
      });
      return request;
    }
  } as unknown as IDBFactory;
}

const VIEW: ProjectView = {
  view: 'chart',
  plateView: 'top',
  selectedColour: null,
  bedPart: null,
  comparePaint: false
};

const write = (id: string) => ({
  id,
  title: 'Motif',
  mode: 'tiles' as const,
  view: { ...VIEW },
  profileId: null,
  file: new Blob(['artwork'], { type: 'application/json' })
});

describe('the saved workspace', () => {
  let store: WorkspaceStore;

  beforeEach(async () => {
    vi.stubGlobal('indexedDB', fakeIndexedDb());
    store = await openWorkspace(
      () => undefined,
      () => undefined,
      () => undefined
    );
  });

  afterEach(() => {
    store.close();
    vi.unstubAllGlobals();
  });

  it('remembers an existing project without advancing the revision', async () => {
    const saved = await store.saveProject(write('p1'), null);
    const before = (await store.snapshot()).revision;
    await store.updatePreferences({ lastProjectId: saved.id });
    const snapshot = await store.snapshot();
    expect(snapshot.preferences.lastProjectId).toBe('p1');
    expect(snapshot.revision).toBe(before);
  });

  it('keeps the current selection when another tab removed the previous project', async () => {
    const first = await store.saveProject(write('p1'), null);
    await store.updatePreferences({ lastProjectId: first.id });
    await store.removeProject(first.id, first.revision);
    const second = await store.saveProject(write('p2'), null);
    await store.updatePreferences({ lastProjectId: second.id });
    const before = (await store.snapshot()).revision;
    // The id was read from a project menu before the removal; remembering it now must not
    // replace the project this workspace actually has open.
    await store.updatePreferences({ lastProjectId: first.id });
    const snapshot = await store.snapshot();
    expect(snapshot.preferences.lastProjectId).toBe('p2');
    expect(snapshot.revision).toBe(before);
  });

  it('does not link a project this workspace never held', async () => {
    await store.updatePreferences({ lastProjectId: 'missing' });
    expect((await store.snapshot()).preferences.lastProjectId).toBeNull();
  });

  it('applies the rest of the patch while refusing a removed project', async () => {
    await store.updatePreferences({ lastProjectId: 'missing', theme: 'dark' });
    const { preferences } = await store.snapshot();
    expect(preferences.theme).toBe('dark');
    expect(preferences.lastProjectId).toBeNull();
  });

  it.each([false, true])(
    'refuses a non-fitting restore choice without writing (restore preferences: %s)',
    async (restorePreferences) => {
      const first = await store.saveProject(write('p1'), null);
      await store.updatePreferences({ lastProjectId: first.id });
      const before = await store.snapshot();
      const printer: PrinterProfile = {
        id: 'pr1',
        revision: 0,
        name: 'Printer',
        widthMm: 200,
        depthMm: 200
      };
      const second = { ...first, ...write('p2'), revision: 0, profileId: printer.id };
      const plan: RestorePlan = {
        revision: before.revision,
        projects: [{ source: second, record: second, action: 'add' }],
        profiles: [{ source: printer, record: printer, action: 'add' }],
        preferences: {
          defaults: before.preferences.defaults,
          theme: 'dark',
          defaultProfileId: printer.id
        },
        fits: { keep: restorePreferences, restore: !restorePreferences }
      };

      await expect(store.restore(plan, restorePreferences)).rejects.toMatchObject({
        code: 'too-large'
      });
      const refused = await store.snapshot();
      expect(refused.revision).toBe(before.revision);
      expect(refused.projects.map(({ id }) => id)).toEqual(['p1']);
      expect(refused.profiles).toEqual([]);
      expect(refused.preferences).toEqual(before.preferences);

      // The opposite choice is safe and the same preview is still current.
      await expect(store.restore(plan, !restorePreferences)).resolves.toBe(true);
      const applied = await store.snapshot();
      expect(applied.revision).toBe(before.revision + 1);
      expect(applied.projects.find(({ id }) => id === 'p2')).toMatchObject({ profileId: 'pr1' });
      expect(applied.profiles).toMatchObject([{ id: 'pr1' }]);
      expect(applied.preferences).toMatchObject({
        theme: restorePreferences ? before.preferences.theme : 'dark',
        defaultProfileId: restorePreferences ? before.preferences.defaultProfileId : 'pr1',
        lastProjectId: first.id
      });
      // A stale preview must ask for a fresh plan, even if its old fit flag was false.
      await expect(store.restore(plan, restorePreferences)).resolves.toBe(false);
      expect((await store.snapshot()).revision).toBe(applied.revision);
    }
  );

  it('clears the selection when asked explicitly', async () => {
    const saved = await store.saveProject(write('p1'), null);
    await store.updatePreferences({ lastProjectId: saved.id });
    await store.updatePreferences({ lastProjectId: null });
    expect((await store.snapshot()).preferences.lastProjectId).toBeNull();
  });

  it('clears the selection when the remembered project is removed', async () => {
    const saved = await store.saveProject(write('p1'), null);
    await store.updatePreferences({ lastProjectId: saved.id });
    await store.removeProject(saved.id, saved.revision);
    expect((await store.snapshot()).preferences.lastProjectId).toBeNull();
  });
});
