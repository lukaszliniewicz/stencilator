import { describe, expect, it, vi } from 'vitest';

// Only the dependency-free helpers are exercised here: the class itself needs the Svelte
// compiler, which the Node unit suite does not load.
import { remoteChange, settleSaves } from './workspace.svelte';
import type { ProjectView, SavedProject } from './workspace-store';

const VIEW: ProjectView = {
  view: 'chart',
  plateView: 'top',
  selectedColour: null,
  bedPart: null,
  comparePaint: false
};

const project = (overrides: Partial<SavedProject> = {}): SavedProject => ({
  id: 'p1',
  revision: 1,
  editedAt: 1_700_000_000_000,
  title: 'Motif',
  mode: 'tiles',
  view: { ...VIEW },
  profileId: null,
  file: new Blob(['artwork']),
  digest: 'aaaa',
  ...overrides
});

/** A promise that is settled by the returned functions, to hold a write in flight. */
function deferred(): {
  promise: Promise<boolean>;
  resolve: (value: boolean) => void;
  reject: (error: unknown) => void;
} {
  let resolve!: (value: boolean) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<boolean>((settle, fail) => {
    resolve = settle;
    reject = fail;
  });
  return { promise, resolve, reject };
}

describe('a project revision another tab committed', () => {
  it('ignores an unchanged revision', () => {
    expect(remoteChange(project(), project(), false, VIEW)).toEqual({ action: 'none' });
  });

  it("adopts a view-only save but keeps this tab's screen", () => {
    const saved = project();
    const other = { ...VIEW, selectedColour: '#102030' };
    const remote = project({ revision: 2, view: other });
    expect(remoteChange(saved, remote, false, VIEW)).toEqual({
      action: 'adopt',
      record: { ...remote, view: VIEW }
    });
  });

  it('adopts a rename, which also leaves the bytes alone', () => {
    const saved = project();
    const remote = project({ revision: 3, title: 'Renamed' });
    expect(remoteChange(saved, remote, false, VIEW)).toEqual({ action: 'adopt', record: remote });
  });

  it('reopens a project whose artwork bytes changed', () => {
    const saved = project();
    const remote = project({ revision: 2, digest: 'bbbb' });
    expect(remoteChange(saved, remote, false, VIEW)).toEqual({ action: 'reopen', record: remote });
  });

  it('treats a pending draft as a conflict, whatever the other tab saved', () => {
    const saved = project();
    expect(remoteChange(saved, project({ revision: 2 }), true, VIEW)).toEqual({
      action: 'conflict'
    });
    expect(remoteChange(saved, project({ revision: 2, digest: 'bbbb' }), true, VIEW)).toEqual({
      action: 'conflict'
    });
  });

  it('treats a project removed elsewhere as a conflict', () => {
    expect(remoteChange(project(), undefined, false, VIEW)).toEqual({ action: 'conflict' });
  });
});

describe('flushing queued saves', () => {
  it('waits for a write already in flight before reporting success', async () => {
    const flight = deferred();
    const settled = vi.fn();
    const flush = settleSaves({
      flight: () => flight.promise,
      pending: () => false,
      blocked: () => false,
      write: () => Promise.resolve(true)
    }).then((value) => {
      settled(value);
      return value;
    });
    await Promise.resolve();
    expect(settled).not.toHaveBeenCalled();
    flight.resolve(true);
    await expect(flush).resolves.toBe(true);
    expect(settled).toHaveBeenCalledWith(true);
  });

  it('reports failure when the write already in flight failed, even with no draft', async () => {
    const flush = settleSaves({
      flight: () => Promise.resolve(false),
      pending: () => false,
      blocked: () => false,
      write: () => Promise.resolve(true)
    });
    await expect(flush).resolves.toBe(false);
  });

  it('writes the draft that is still pending after the earlier write settles', async () => {
    let pending = true;
    const write = vi.fn(() => {
      pending = false;
      return Promise.resolve(true);
    });
    const flush = settleSaves({
      flight: () => Promise.resolve(true),
      pending: () => pending,
      blocked: () => false,
      write
    });
    await expect(flush).resolves.toBe(true);
    expect(write).toHaveBeenCalledTimes(1);
  });

  it('stops after a failed write', async () => {
    const write = vi.fn(() => Promise.resolve(false));
    const flush = settleSaves({
      flight: () => undefined,
      pending: () => true,
      blocked: () => false,
      write
    });
    await expect(flush).resolves.toBe(false);
    expect(write).toHaveBeenCalledTimes(1);
  });

  it('does not save while an upgrade or conflict blocks it', async () => {
    const write = vi.fn(() => Promise.resolve(true));
    const flush = settleSaves({
      flight: () => undefined,
      pending: () => true,
      blocked: () => true,
      write
    });
    await expect(flush).resolves.toBe(false);
    expect(write).not.toHaveBeenCalled();
  });
});
