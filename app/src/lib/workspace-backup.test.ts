import { assert, describe, expect, it, vi } from 'vitest';

import { sanitise } from './settings';
import { decodeBackup, encodeBackup, previewRestore } from './workspace-backup';
import type { PrinterProfile, SavedProject, WorkspaceSnapshot } from './workspace-store';

/** The engine's validation is mocked: any declared "tiles" project opens. */
const validateProject = (): Promise<'tiles'> => Promise.resolve('tiles');

const sha256 = async (text: string): Promise<string> => {
  const hash = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return Array.from(new Uint8Array(hash), (byte) => byte.toString(16).padStart(2, '0')).join('');
};

const VIEW = {
  view: 'chart',
  plateView: 'top',
  selectedColour: null,
  bedPart: null,
  comparePaint: false
} as const;

const project = async (
  id: string,
  title: string,
  content: string,
  extra: Partial<SavedProject> = {}
): Promise<SavedProject> => ({
  id,
  revision: 1,
  editedAt: 1_700_000_000_000,
  title,
  mode: 'tiles',
  view: { ...VIEW },
  profileId: null,
  file: new Blob([content], { type: 'application/json' }),
  digest: await sha256(content),
  ...extra
});

const profile = (
  id: string,
  name: string,
  widthMm = 200,
  depthMm = 200,
  extra: Partial<PrinterProfile> = {}
): PrinterProfile => ({
  id,
  revision: 1,
  name,
  widthMm,
  depthMm,
  ...extra
});

const preferences = (overrides: Partial<WorkspaceSnapshot['preferences']> = {}) => ({
  defaults: sanitise(undefined),
  theme: 'auto' as const,
  defaultProfileId: null,
  lastProjectId: null,
  ...overrides
});

const wireProject = (id: string, mode = 'tiles', profileId: string | null = null) => ({
  id,
  revision: 1,
  editedAt: 1_700_000_000_000,
  title: id,
  mode,
  view: { ...VIEW },
  profileId,
  file: btoa('bytes'),
  digest: ''
});

describe('workspace backups', () => {
  it('round-trips projects, profiles and preferences without touching the digest', async () => {
    const original = await project('p1', 'One', '{"a":1}', { profileId: 'pr1' });
    const profiles = [profile('pr1', 'Ender')];
    const snapshot: WorkspaceSnapshot = {
      revision: 3,
      projects: [original],
      profiles,
      preferences: preferences({ theme: 'dark', defaultProfileId: 'pr1', lastProjectId: 'p1' })
    };

    const backup = await decodeBackup(await encodeBackup(snapshot), validateProject);
    expect(backup.version).toBe(1);
    expect(backup.projects).toHaveLength(1);
    const [restored] = backup.projects;
    assert(restored);
    expect(restored.digest).toBe(original.digest);
    expect(await restored.file.text()).toBe('{"a":1}');
    expect(restored.profileId).toBe('pr1');
    expect(backup.profiles).toMatchObject([{ name: 'Ender' }]);
    expect(backup.preferences.theme).toBe('dark');
    expect(backup.preferences.defaultProfileId).toBe('pr1');
    // The last open project is not part of a backup.
    expect(JSON.parse(await (await encodeBackup(snapshot)).text())).not.toHaveProperty(
      'preferences.lastProjectId'
    );
  });

  it('rejects damaged, unsupported and inconsistent backups', async () => {
    await expect(decodeBackup(new Blob(['not json']), validateProject)).rejects.toMatchObject({
      code: 'invalid'
    });
    await expect(
      decodeBackup(new Blob([JSON.stringify({ stencilator_workspace: 2 })]), validateProject)
    ).rejects.toMatchObject({ code: 'unsupported' });

    const envelope = (projects: unknown[], profiles: unknown[] = []) =>
      JSON.stringify({
        stencilator_workspace: 1,
        projects,
        profiles,
        preferences: { theme: 'auto', defaultProfileId: null, defaults: sanitise(undefined) }
      });

    await expect(
      decodeBackup(new Blob([envelope([wireProject('p1'), wireProject('p1')])]), validateProject)
    ).rejects.toMatchObject({ code: 'invalid' });

    await expect(
      decodeBackup(new Blob([envelope([wireProject('p1', 'zigzag')])]), validateProject)
    ).rejects.toMatchObject({ code: 'invalid' });

    await expect(
      decodeBackup(
        new Blob([envelope([wireProject('p1', 'tiles', 'missing-profile')])]),
        validateProject
      )
    ).rejects.toMatchObject({ code: 'invalid' });
  });

  it('rejects too many projects before validating any of them', async () => {
    const validate = vi.fn(validateProject);
    const file = new Blob([
      JSON.stringify({
        stencilator_workspace: 1,
        projects: Array.from({ length: 501 }, (_, index) => wireProject(`p${String(index)}`)),
        profiles: [],
        preferences: preferences()
      })
    ]);

    await expect(decodeBackup(file, validate)).rejects.toMatchObject({ code: 'too-large' });
    expect(validate).not.toHaveBeenCalled();
  });

  it('skips already-imported copies on a repeat restore', async () => {
    const userProject = await project('p1', 'User project', 'user-bytes');
    const importedCopy = await project('p1-copy', 'One', 'shared-bytes', {
      origin: { id: 'p1', name: 'One', importedAs: 'One' }
    });
    const snapshot: WorkspaceSnapshot = {
      revision: 1,
      projects: [userProject, importedCopy],
      profiles: [],
      preferences: preferences()
    };
    const backup = {
      version: 1 as const,
      projects: [await project('p1', 'One', 'shared-bytes')],
      profiles: [],
      preferences: preferences()
    };

    const plan = previewRestore(backup, snapshot);
    expect(plan.projects).toMatchObject([{ action: 'skip', record: importedCopy }]);
  });

  it('skips unchanged copies even when the backup was made after restoring them', async () => {
    const restoredProfile = profile('pr1-copy', 'Ender (copy)', 240, 210, {
      origin: { id: 'pr1', name: 'Ender', importedAs: 'Ender (copy)' }
    });
    const restoredProject = await project('p1-copy', 'Fox (copy)', 'bytes', {
      origin: { id: 'p1', name: 'Fox', importedAs: 'Fox (copy)' },
      profileId: restoredProfile.id
    });
    const snapshot: WorkspaceSnapshot = {
      revision: 4,
      projects: [restoredProject],
      profiles: [restoredProfile],
      preferences: preferences({ defaultProfileId: restoredProfile.id })
    };

    const backup = await decodeBackup(await encodeBackup(snapshot), validateProject);
    const plan = previewRestore(backup, snapshot);
    expect(plan.projects).toMatchObject([{ action: 'skip', record: restoredProject }]);
    expect(plan.profiles).toMatchObject([{ action: 'skip', record: restoredProfile }]);
    expect(plan.preferences.defaultProfileId).toBe(restoredProfile.id);
  });

  it('copies a changed record and adds a new one with a distinct title', async () => {
    const existing = await project('p1', 'Fox', 'old-bytes');
    const snapshot: WorkspaceSnapshot = {
      revision: 2,
      projects: [existing],
      profiles: [],
      preferences: preferences()
    };
    const edited = await project('p1', 'Fox', 'new-bytes');
    const backup = {
      version: 1 as const,
      projects: [edited, await project('p2', 'Fox', 'other-bytes')],
      profiles: [],
      preferences: preferences()
    };

    const plan = previewRestore(backup, snapshot);
    const [changed, added] = plan.projects;
    assert(changed);
    assert(added);
    expect(changed.action).toBe('copy');
    expect(changed.record.id).not.toBe('p1');
    expect(changed.record.digest).toBe(edited.digest);
    expect(added.action).toBe('add');
    expect(added.record.id).toBe('p2');
    // Matching by name alone never overwrites: the added "Fox" gets a distinct title.
    expect(added.record.title.toLocaleLowerCase()).not.toBe('fox');
  });

  it('repoints a project to a profile imported as a copy', async () => {
    const existingProfile = profile('pr1', 'Ender', 200, 200);
    const snapshot: WorkspaceSnapshot = {
      revision: 3,
      projects: [],
      profiles: [existingProfile],
      preferences: preferences({ defaultProfileId: 'pr1' })
    };
    const backup = {
      version: 1 as const,
      projects: [await project('p1', 'One', 'bytes', { profileId: 'pr1' })],
      // Same id but different dimensions, so the profile is imported as a copy.
      profiles: [profile('pr1', 'Ender', 300, 300)],
      preferences: preferences({ defaultProfileId: 'pr1' })
    };

    const plan = previewRestore(backup, snapshot);
    const [copied] = plan.profiles;
    assert(copied);
    expect(copied.action).toBe('copy');
    const newProfileId = copied.record.id;
    expect(newProfileId).not.toBe('pr1');
    expect(plan.projects).toMatchObject([{ record: { profileId: newProfileId } }]);
    expect(plan.preferences.defaultProfileId).toBe(newProfileId);
  });
});
