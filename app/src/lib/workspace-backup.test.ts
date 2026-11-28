import { assert, describe, expect, it, vi } from 'vitest';

import { sanitise } from './settings';
import { decodeBackup, encodeBackup, planBackups, previewRestore } from './workspace-backup';
import {
  WorkspaceError,
  type PrinterProfile,
  type SavedProject,
  type WorkspaceSnapshot
} from './workspace-store';

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

/** Three copies, the record and both origin fields, exceed the 160 MiB export budget. */
const HUGE_NAME = 'a'.repeat(54 * 1024 * 1024);

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

const defaultsBackup = (defaults: unknown): Blob =>
  new Blob([
    JSON.stringify({
      stencilator_workspace: 1,
      projects: [],
      profiles: [],
      preferences: { theme: 'auto', defaultProfileId: null, defaults }
    })
  ]);

const backupFile = (overrides: Record<string, unknown> = {}): Blob =>
  new Blob([
    JSON.stringify({
      stencilator_workspace: 1,
      projects: [],
      profiles: [],
      preferences: { theme: 'auto', defaultProfileId: null, defaults: sanitise(undefined) },
      ...overrides
    })
  ]);

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
  it('round-trips records and recomputes the digest from the project bytes', async () => {
    const original = await project('p1', 'One', '{"a":1}', { profileId: 'pr1' });
    const profiles = [profile('pr1', 'Ender')];
    const snapshot: WorkspaceSnapshot = {
      revision: 3,
      projects: [original],
      profiles,
      preferences: preferences({ theme: 'dark', defaultProfileId: 'pr1', lastProjectId: 'p1' })
    };

    // The stored fingerprint is untrusted: the decoded one is recomputed from the bytes.
    const encoded = JSON.parse(await (await encodeBackup(snapshot)).text()) as {
      projects: { digest: string }[];
    };
    const wire = encoded.projects[0];
    assert(wire);
    wire.digest = 'tampered';
    const backup = await decodeBackup(
      new Blob([JSON.stringify(encoded)], { type: 'application/json' }),
      validateProject
    );
    expect(backup.version).toBe(1);
    expect(backup.projects).toHaveLength(1);
    const [restored] = backup.projects;
    assert(restored);
    expect(restored.digest).toBe(await sha256(await restored.file.text()));
    expect(restored.digest).toBe(original.digest);
    expect(restored.digest).not.toBe('tampered');
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

  it('requires a part’s records to be included in its list of exported IDs', async () => {
    const validate = vi.fn(validateProject);
    const file = new Blob([
      JSON.stringify({
        stencilator_workspace: 1,
        projects: [wireProject('p1')],
        profiles: [],
        preferences: preferences(),
        exportedIds: { projects: ['elsewhere'], profiles: [] }
      })
    ]);

    await expect(decodeBackup(file, validate)).rejects.toMatchObject({ code: 'invalid' });
    expect(validate).not.toHaveBeenCalled();
  });

  it('distinguishes an unavailable validator from an invalid project', async () => {
    const snapshot: WorkspaceSnapshot = {
      revision: 1,
      projects: [await project('p1', 'One', 'bytes')],
      profiles: [],
      preferences: preferences()
    };
    const file = await encodeBackup(snapshot);
    await expect(
      decodeBackup(file, () => Promise.reject(new Error('Fonts unavailable')))
    ).rejects.toMatchObject({ code: 'validation-unavailable' });
    await expect(decodeBackup(file, () => Promise.resolve(null))).rejects.toMatchObject({
      code: 'invalid'
    });
    await expect(decodeBackup(file, () => Promise.resolve('freeform'))).rejects.toMatchObject({
      code: 'invalid'
    });
  });

  it('splits large workspaces without losing projects, unlinked profiles or printer links', async () => {
    const shared = profile('shared', 'Textured sheet');
    const defaultPrinter = profile('default', 'Smooth sheet');
    const design = await project('p0', 'One', 'bytes', { profileId: shared.id });
    const snapshot: WorkspaceSnapshot = {
      revision: 1,
      projects: Array.from({ length: 501 }, (_, i) => ({
        ...design,
        id: `p${String(i)}`,
        title: `Project ${String(i)}`
      })),
      profiles: [
        shared,
        defaultPrinter,
        ...Array.from({ length: 500 }, (_, i) =>
          profile(`other-${String(i)}`, `Other ${String(i)}`)
        )
      ],
      preferences: preferences({ theme: 'dark', defaultProfileId: defaultPrinter.id })
    };
    const { parts, excluded } = planBackups(snapshot);
    expect(excluded).toEqual([]);
    expect(parts.map(({ projects }) => projects.length)).toEqual([500, 1, 0]);
    const restored: WorkspaceSnapshot = {
      revision: 0,
      projects: [],
      profiles: [],
      preferences: preferences()
    };
    const [, middlePart] = parts;
    assert(middlePart);
    // Each file works on its own, including when restored out of order or twice.
    for (const part of [middlePart, ...parts.toReversed()]) {
      const backup = await decodeBackup(await encodeBackup(part), validateProject);
      const plan = previewRestore(backup, restored);
      restored.projects.push(
        ...plan.projects.filter(({ action }) => action !== 'skip').map(({ record }) => record)
      );
      restored.profiles.push(
        ...plan.profiles.filter(({ action }) => action !== 'skip').map(({ record }) => record)
      );
      expect(plan.preferences.theme).toBe('dark');
      expect(plan.preferences.defaultProfileId).toBe(defaultPrinter.id);
    }
    expect(restored.profiles.map(({ id }) => id).sort()).toEqual(
      snapshot.profiles.map(({ id }) => id).sort()
    );
    expect(restored.projects.map(({ id }) => id).sort()).toEqual(
      snapshot.projects.map(({ id }) => id).sort()
    );
    expect(restored.projects.every(({ profileId }) => profileId === shared.id)).toBe(true);
    for (const part of parts) {
      const backup = await decodeBackup(await encodeBackup(part), validateProject);
      const plan = previewRestore(backup, restored);
      expect([...plan.projects, ...plan.profiles].every(({ action }) => action === 'skip')).toBe(
        true
      );
    }
  });

  it('splits by the total project bytes and excludes only an oversized project', async () => {
    const original = await project('p', 'One', 'bytes');
    const sizedFile = (size: number): Blob => {
      const file = new Blob();
      Object.defineProperty(file, 'size', { value: size });
      return file;
    };
    const size = 32 * 1024 * 1024;
    const printer = profile('pr1', 'Ender');
    const snapshot: WorkspaceSnapshot = {
      revision: 1,
      projects: [
        { ...original, id: 'large', title: 'Too large', file: sizedFile(size + 1) },
        ...Array.from({ length: 4 }, (_, i) => ({
          ...original,
          id: `p${String(i)}`,
          // 96 MiB plus one byte, still well below the encoded-file ceiling.
          file: sizedFile(i === 3 ? 1 : size)
        }))
      ],
      profiles: [printer],
      preferences: preferences({ defaultProfileId: printer.id })
    };
    expect(planBackups(snapshot)).toMatchObject({
      excluded: [{ id: 'large', title: 'Too large' }],
      parts: [
        { projects: [{ id: 'p0' }, { id: 'p1' }, { id: 'p2' }], profiles: [printer] },
        { projects: [{ id: 'p3' }], profiles: [printer] }
      ]
    });
    expect(planBackups({ ...snapshot, projects: snapshot.projects.slice(0, 1) })).toMatchObject({
      excluded: [{ id: 'large', title: 'Too large' }],
      parts: [{ projects: [], profiles: [printer], preferences: snapshot.preferences }]
    });
  });

  it.each(['original first', 'copy first'])(
    'keeps identical saved copies across backup files (%s)',
    async (order) => {
      const defaultPrinter = profile('pr0', 'Smooth sheet');
      const printer = profile('pr1', 'Ender');
      const copiedPrinter = {
        ...printer,
        id: 'pr1-copy',
        name: 'Ender (copy)',
        origin: { id: printer.id, name: printer.name, importedAs: 'Ender (copy)' }
      };
      const fox = await project('p1', 'Fox', 'bytes', { profileId: defaultPrinter.id });
      const copiedFox = {
        ...fox,
        id: 'p1-copy',
        title: 'Fox (copy)',
        origin: { id: fox.id, name: fox.title, importedAs: 'Fox (copy)' }
      };
      const restored: WorkspaceSnapshot = {
        revision: 0,
        projects: [],
        profiles: [],
        preferences: preferences()
      };
      const parts = await Promise.all(
        [
          { design: fox, linked: printer },
          { design: copiedFox, linked: copiedPrinter }
        ].map(async ({ design, linked }) =>
          encodeBackup({
            revision: 4,
            projects: [
              design,
              await project(`sign-${linked.id}`, `Sign for ${linked.name}`, 'sign', {
                profileId: linked.id
              })
            ],
            profiles: [defaultPrinter, linked],
            preferences: preferences({ defaultProfileId: defaultPrinter.id })
          })
        )
      );
      if (order === 'copy first') parts.reverse();
      for (const part of parts) {
        const plan = previewRestore(await decodeBackup(part, validateProject), restored);
        expect(plan.projects.map(({ action }) => action)).toEqual(['add', 'add']);
        restored.projects.push(...plan.projects.map(({ record }) => record));
        restored.profiles.push(
          ...plan.profiles.filter(({ action }) => action !== 'skip').map(({ record }) => record)
        );
      }
      expect(restored.profiles.map(({ id }) => id).sort()).toEqual(['pr0', 'pr1', 'pr1-copy']);
      expect(restored.projects).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ id: fox.id, profileId: defaultPrinter.id }),
          expect.objectContaining({ id: copiedFox.id, profileId: defaultPrinter.id }),
          expect.objectContaining({ id: 'sign-pr1', profileId: printer.id }),
          expect.objectContaining({ id: 'sign-pr1-copy', profileId: copiedPrinter.id })
        ])
      );
      for (const part of parts) {
        const plan = previewRestore(await decodeBackup(part, validateProject), restored);
        expect([...plan.projects, ...plan.profiles].map(({ action }) => action)).toEqual([
          'skip',
          'skip',
          'skip',
          'skip'
        ]);
      }
    }
  );

  it('recognises a repeat restore when that saved copy needed a new id', async () => {
    const imported = await project('p1-copy', 'Fox (copy)', 'bytes', {
      origin: { id: 'p1', name: 'Fox', importedAs: 'Fox (copy)' }
    });
    const local = await project(imported.id, 'Local project', 'other bytes');
    const snapshot: WorkspaceSnapshot = {
      revision: 2,
      projects: [local],
      profiles: [],
      preferences: preferences()
    };
    const backup = await decodeBackup(
      await encodeBackup({ ...snapshot, projects: [imported] }),
      validateProject
    );
    const [entry] = previewRestore(backup, snapshot).projects;
    assert(entry);
    expect(entry.action).toBe('copy');
    expect(entry.record.id).not.toBe(imported.id);
    snapshot.projects.push(entry.record);
    // An unrelated record restored from the common origin must still get its own entry.
    const original = await project('p1', 'Fox', 'bytes');
    const otherBackup = { ...backup, projects: [original] };
    expect(previewRestore(otherBackup, snapshot).projects).toMatchObject([{ action: 'add' }]);
    expect(previewRestore(backup, snapshot).projects).toMatchObject([
      { action: 'skip', record: entry.record }
    ]);
  });

  it.each(['original first', 'copy first'])(
    'restores a missing original beside its identical saved copy in a different part (%s)',
    async (order) => {
      const printer = profile('pr1', 'Ender');
      const original = await project('p1', 'Fox', 'bytes', { profileId: printer.id });
      const snapshot: WorkspaceSnapshot = {
        revision: 1,
        projects: [await project(original.id, original.title, 'old bytes')],
        profiles: [profile(printer.id, printer.name, 300)],
        preferences: preferences()
      };
      const incoming = {
        version: 1 as const,
        projects: [original],
        profiles: [printer],
        preferences: preferences()
      };
      const { projects, profiles } = previewRestore(incoming, snapshot);
      const [projectCopy] = projects;
      const [profileCopy] = profiles;
      assert(projectCopy?.action === 'copy' && profileCopy?.action === 'copy');

      // Both originals now match the copies. Put each project and its linked
      // profile in a different numbered backup file.
      const fillers = Array.from({ length: 499 }, (_, i) => ({
        ...original,
        id: `extra-${String(i)}`,
        title: `Extra ${String(i)}`
      }));
      const { parts } = planBackups({
        ...snapshot,
        projects: [original, ...fillers, projectCopy.record],
        profiles: [printer, profileCopy.record]
      });
      expect(parts.map((part) => part.projects.length)).toEqual([500, 1]);
      const backups = await Promise.all(
        parts.map(async (part) => decodeBackup(await encodeBackup(part), validateProject))
      );
      if (order === 'copy first') backups.reverse();

      // Only the copies are still in the browser when the backup is restored.
      snapshot.projects = [projectCopy.record];
      snapshot.profiles = [profileCopy.record];
      for (const backup of backups) {
        const plan = previewRestore(backup, snapshot);
        snapshot.projects.push(
          ...plan.projects.filter(({ action }) => action !== 'skip').map(({ record }) => record)
        );
        snapshot.profiles.push(
          ...plan.profiles.filter(({ action }) => action !== 'skip').map(({ record }) => record)
        );
      }
      expect(snapshot.projects).toHaveLength(501);
      expect(snapshot.projects.find(({ id }) => id === original.id)?.profileId).toBe(printer.id);
      expect(snapshot.profiles.map(({ id }) => id)).toEqual([profileCopy.record.id, printer.id]);
      for (const backup of backups) {
        const plan = previewRestore(backup, snapshot);
        expect([...plan.projects, ...plan.profiles].every(({ action }) => action === 'skip')).toBe(
          true
        );
      }
    }
  );

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

  it('keeps the backed-up names of copies renamed locally and skips repeat restores', async () => {
    const restoredProject = await project('p1-copy', 'Fox (copy)', 'bytes', {
      origin: { id: 'p1', sourceId: 'p1', name: 'Fox', importedAs: 'Fox (copy)' }
    });
    const restoredProfile = profile('pr1-copy', 'Ender (copy)', 240, 210, {
      origin: { id: 'pr1', sourceId: 'pr1', name: 'Ender', importedAs: 'Ender (copy)' }
    });
    const snapshot: WorkspaceSnapshot = {
      revision: 4,
      projects: [restoredProject],
      profiles: [restoredProfile],
      preferences: preferences()
    };
    const backup = await decodeBackup(await encodeBackup(snapshot), validateProject);

    snapshot.projects = [
      { ...restoredProject, title: 'Fox' },
      await project('other-project', restoredProject.title, 'other bytes')
    ];
    snapshot.profiles = [
      { ...restoredProfile, name: 'Ender' },
      profile('other-profile', restoredProfile.name)
    ];
    const plan = previewRestore(backup, snapshot);
    const [design] = plan.projects;
    const [printer] = plan.profiles;
    assert(design && printer);
    expect(design).toMatchObject({ action: 'copy', record: { title: 'Fox (copy) (copy)' } });
    expect(printer).toMatchObject({ action: 'copy', record: { name: 'Ender (copy) (copy)' } });
    snapshot.projects.push(design.record);
    snapshot.profiles.push(printer.record);

    const repeat = previewRestore(backup, snapshot);
    expect(repeat.projects).toMatchObject([{ action: 'skip', record: design.record }]);
    expect(repeat.profiles).toMatchObject([{ action: 'skip', record: printer.record }]);
  });

  it('recognises an earlier restore that kept the ancestor name in its origin', async () => {
    const imported = await project('p1-copy', 'Fox (copy)', 'bytes', {
      origin: { id: 'p1', name: 'Fox', importedAs: 'Fox (copy)' }
    });
    const restored = {
      ...imported,
      title: 'Fox (copy) (copy 2)',
      origin: { id: 'p1', sourceId: imported.id, name: 'Fox', importedAs: 'Fox (copy) (copy 2)' }
    };
    const snapshot: WorkspaceSnapshot = {
      revision: 4,
      projects: [restored],
      profiles: [],
      preferences: preferences()
    };
    const backup = {
      version: 1 as const,
      projects: [imported],
      profiles: [],
      preferences: preferences()
    };
    expect(previewRestore(backup, snapshot).projects).toMatchObject([
      { action: 'skip', record: restored }
    ]);
    backup.projects = [{ ...imported, title: 'Fox' }];
    expect(previewRestore(backup, snapshot).projects).toMatchObject([
      { action: 'copy', record: { title: 'Fox' } }
    ]);
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
    const titles = [existing.title, changed.record.title, added.record.title];
    expect(new Set(titles.map((title) => title.toLocaleLowerCase())).size).toBe(titles.length);
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

  it('repairs restored defaults the engine would reject and keeps valid values', async () => {
    const file = new Blob([
      JSON.stringify({
        stencilator_workspace: 1,
        projects: [],
        profiles: [],
        preferences: {
          theme: 'auto',
          defaultProfileId: null,
          defaults: {
            canvasWidthMm: -5,
            canvasHeightMm: 300,
            tileMm: 0.05,
            dpi: 5_000,
            colours: 300,
            cellMm: 3,
            minPatchMm2: 150,
            marginMm: 1,
            minWebMm: 1.1,
            bridgeWidthMm: 8,
            connectorClearanceMm: 1,
            thicknessMm: 50,
            bedWidthMm: 20,
            // Project state, not a workspace default: `sanitise` clears it before ranges.
            registrationClearanceMm: 2,
            chamferMm: 3,
            overlapMm: 3
          }
        }
      })
    ]);

    const { preferences } = await decodeBackup(file, validateProject);
    expect(preferences.defaults).toMatchObject({
      canvasWidthMm: 200,
      canvasHeightMm: 300,
      tileMm: 5,
      dpi: 300,
      colours: 16,
      cellMm: 0.1,
      minPatchMm2: 1,
      marginMm: 10,
      minWebMm: 1.1,
      bridgeWidthMm: 1.2,
      connectorClearanceMm: 4,
      thicknessMm: 1.2,
      bedWidthMm: 256,
      registrationClearanceMm: null,
      chamferMm: 0,
      overlapMm: 0
    });
    // Existing `sanitise` resets project-specific settings; the restore does not range-check it.
    expect(sanitise({ registrationClearanceMm: 2 }).registrationClearanceMm).toBeNull();

    // A field left out still takes the engine's default.
    const sparse = new Blob([
      JSON.stringify({
        stencilator_workspace: 1,
        projects: [],
        profiles: [],
        preferences: { theme: 'auto', defaultProfileId: null, defaults: { cellMm: 0.2 } }
      })
    ]);
    const decoded = await decodeBackup(sparse, validateProject);
    expect(decoded.preferences.defaults).toEqual({ ...sanitise(undefined), cellMm: 0.2 });
  });

  it('keeps the cross-field defaults the engine accepts at the micrometre boundary', async () => {
    const accepted = await decodeBackup(
      defaultsBackup({ thicknessMm: 1.2, connectorClearanceMm: 1.4, chamferMm: 1 }),
      validateProject
    );
    expect(accepted.preferences.defaults).toMatchObject({
      thicknessMm: 1.2,
      connectorClearanceMm: 1.4,
      chamferMm: 1
    });

    const tight = await decodeBackup(
      defaultsBackup({ thicknessMm: 1.2, connectorClearanceMm: 1.399, chamferMm: 1.1 }),
      validateProject
    );
    expect(tight.preferences.defaults).toMatchObject({
      thicknessMm: 1.2,
      connectorClearanceMm: 4,
      chamferMm: 0
    });
  });

  it('checks defaults for every mode, not only the saved one', async () => {
    const { preferences } = await decodeBackup(
      defaultsBackup({
        sizing: 'pixels',
        canvasWidthMm: -5,
        sizeMode: 'natural',
        artWidthMm: -5,
        supportKind: 'none',
        bridgeWidthMm: 8
      }),
      validateProject
    );
    expect(preferences.defaults).toMatchObject({
      sizing: 'pixels',
      canvasWidthMm: 200,
      sizeMode: 'natural',
      artWidthMm: 200,
      supportKind: 'none',
      bridgeWidthMm: 1.2
    });
  });

  it('keeps paint merges and skipped paints exactly as saved', async () => {
    const hex = (value: number): string => `#${value.toString(16).padStart(6, '0')}`;
    const merges = Array.from({ length: 400 }, (_, i): [string, string] => [
      hex(i),
      hex(i + 1_000)
    ]);
    const skipped = Array.from({ length: 400 }, (_, i) => hex(i + 2_000));
    const { preferences } = await decodeBackup(
      defaultsBackup({ merges, skipped }),
      validateProject
    );
    expect(preferences.defaults.merges).toEqual(merges);
    expect(preferences.defaults.skipped).toEqual(skipped);
  });

  it('refuses restored metadata that could never fit a backup file', () => {
    const backup = {
      version: 1 as const,
      projects: [],
      profiles: [profile('huge', HUGE_NAME)],
      preferences: preferences()
    };
    const snapshot: WorkspaceSnapshot = {
      revision: 0,
      projects: [],
      profiles: [],
      preferences: preferences()
    };

    let thrown: unknown;
    try {
      previewRestore(backup, snapshot);
    } catch (error) {
      thrown = error;
    }
    expect(thrown).toBeInstanceOf(WorkspaceError);
    expect(thrown).toMatchObject({ code: 'too-large' });
  });

  it('exports the other records and names an oversized profile and its linked project', async () => {
    const huge = profile('huge', HUGE_NAME, 200, 200, {
      origin: { id: 'huge', name: HUGE_NAME, importedAs: HUGE_NAME }
    });
    const printer = profile('pr1', 'Ender');
    const linked = await project('p1', 'Linked to the huge profile', 'bytes', {
      profileId: huge.id
    });
    const design = await project('p2', 'One', 'bytes', { profileId: printer.id });
    const snapshot: WorkspaceSnapshot = {
      revision: 5,
      projects: [linked, design],
      profiles: [huge, printer],
      preferences: preferences({ defaultProfileId: printer.id })
    };

    const plan = planBackups(snapshot);
    expect(plan.excluded).toEqual([
      { kind: 'profile', id: 'huge', title: HUGE_NAME },
      { kind: 'project', id: linked.id, title: linked.title }
    ]);
    expect(plan.parts).toHaveLength(1);
    const [part] = plan.parts;
    assert(part);
    expect(part.projects).toEqual([design]);
    expect(part.profiles).toEqual([printer]);
    expect(part.preferences.defaultProfileId).toBe(printer.id);
  });

  it('accepts ids at the length limit in every id field', async () => {
    const id = 'x'.repeat(256);
    const origin = { id, sourceId: id, name: 'Saved', importedAs: 'Saved' };
    const file = backupFile({
      projects: [{ ...wireProject(id, 'tiles', id), origin }],
      profiles: [profile(id, 'Printer', 200, 200, { origin })],
      preferences: { theme: 'auto', defaultProfileId: id, defaults: sanitise(undefined) },
      exportedIds: { projects: [id], profiles: [id] }
    });

    const decoded = await decodeBackup(file, validateProject);
    expect(decoded.projects[0]).toMatchObject({ id, profileId: id, origin });
    expect(decoded.profiles[0]).toMatchObject({ id, origin });
    expect(decoded.preferences.defaultProfileId).toBe(id);
  });

  it.each(
    (() => {
      const overlong = 'x'.repeat(257);
      const atLimit = 'x'.repeat(256);
      const cases: [string, Record<string, unknown>][] = [
        ['project id', { projects: [wireProject(overlong)] }],
        ['profile id', { profiles: [profile(overlong, 'Printer')] }],
        [
          'project profile link',
          {
            projects: [wireProject('p1', 'tiles', overlong)],
            profiles: [profile(atLimit, 'Printer')]
          }
        ],
        [
          'default profile link',
          {
            profiles: [profile('pr1', 'Printer')],
            preferences: {
              theme: 'auto',
              defaultProfileId: overlong,
              defaults: sanitise(undefined)
            }
          }
        ],
        [
          'exported id',
          {
            projects: [wireProject('p1')],
            exportedIds: { projects: ['p1', overlong], profiles: [] }
          }
        ],
        [
          'origin id',
          {
            projects: [
              { ...wireProject('p1'), origin: { id: overlong, name: 'Saved', importedAs: 'Saved' } }
            ]
          }
        ],
        [
          'origin source id',
          {
            projects: [
              {
                ...wireProject('p1'),
                origin: { id: 'p1', sourceId: overlong, name: 'Saved', importedAs: 'Saved' }
              }
            ]
          }
        ]
      ];
      return cases;
    })()
  )('rejects an overlong %s', async (_label, overrides) => {
    await expect(decodeBackup(backupFile(overrides), validateProject)).rejects.toMatchObject({
      code: 'invalid'
    });
  });

  it('excludes a local record whose restored form cannot fit a backup file', async () => {
    // The name alone fits an exported record; its restored origin and copy headroom do not.
    const huge = profile('huge', HUGE_NAME);
    const printer = profile('pr1', 'Ender');
    const linked = await project('p1', 'Linked project', 'bytes', { profileId: huge.id });
    const design = await project('p2', 'Design', 'bytes', { profileId: printer.id });
    const snapshot: WorkspaceSnapshot = {
      revision: 5,
      projects: [linked, design],
      profiles: [huge, printer],
      preferences: preferences({ defaultProfileId: printer.id })
    };

    const { parts, excluded } = planBackups(snapshot);
    expect(excluded).toEqual([
      { kind: 'profile', id: huge.id, title: HUGE_NAME },
      { kind: 'project', id: linked.id, title: linked.title }
    ]);
    expect(parts).toHaveLength(1);
    const [part] = parts;
    assert(part);
    expect(part.projects).toEqual([design]);
    expect(part.profiles).toEqual([printer]);

    // The remaining part still imports cleanly, so the exclusion did not poison it.
    const backup = await decodeBackup(await encodeBackup(part), validateProject);
    expect(
      previewRestore(backup, { ...snapshot, projects: [], profiles: [] }).profiles
    ).toMatchObject([{ action: 'add', record: { id: printer.id, name: printer.name } }]);
  });

  it('drops an unusable default profile and its links but exports the rest', async () => {
    const huge = profile('huge', HUGE_NAME, 200, 200, {
      origin: { id: 'huge', name: HUGE_NAME, importedAs: HUGE_NAME }
    });
    const printer = profile('pr1', 'Ender');
    const linked = await project('p1', 'Linked to the huge profile', 'bytes', {
      profileId: huge.id
    });
    const design = await project('p2', 'One', 'bytes', { profileId: printer.id });
    const snapshot: WorkspaceSnapshot = {
      revision: 5,
      projects: [linked, design],
      profiles: [huge, printer],
      preferences: preferences({ defaultProfileId: huge.id })
    };

    const { parts, excluded } = planBackups(snapshot);
    expect(excluded).toEqual([
      { kind: 'profile', id: 'huge', title: HUGE_NAME },
      { kind: 'project', id: linked.id, title: linked.title }
    ]);
    expect(parts).toHaveLength(1);
    const [part] = parts;
    assert(part);
    expect(part.projects).toEqual([design]);
    expect(part.profiles).toEqual([printer]);
    expect(part.preferences.defaultProfileId).toBeNull();
  });
});
