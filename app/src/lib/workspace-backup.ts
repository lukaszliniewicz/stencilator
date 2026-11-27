/**
 * Portable workspace backups and the restore preview built from them. Each file is a
 * versioned JSON envelope carrying each project's exact `.stencilator` bytes as base64.
 * Decoding validates everything before any write: size, envelope version, unique ids,
 * finite timestamps, profile dimensions within the engine's bed range, view shape, project
 * mode against the engine's own validation, and project-to-profile links. The restore
 * preview is deterministic: it skips, adds or copies by identity and saved content, never
 * by name alone, and remaps linked profile ids while the applied bed stays inside the
 * preserved project bytes.
 */
import type { ProjectMode } from '../engine/client';
import { sanitise } from './settings';
import { text } from './text';
import type { RestoreEntry, RestorePlan } from './workspace-store';
import { WorkspaceError } from './workspace-store';
import type {
  Origin,
  PrinterProfile,
  SavedProject,
  WorkspacePreferences,
  WorkspaceSnapshot
} from './workspace-store';

const BACKUP_FORMAT = 'stencilator_workspace';
const BACKUP_VERSION = 1;

/** The maximum number of projects, and separately of profiles, in a backup. */
const MAX_ITEMS = 500;
/** Each project's `.stencilator` file, in bytes, at most 32 MiB. */
const MAX_PROJECT_BYTES = 32 * 1024 * 1024;
/** All project bytes in one backup, at most 96 MiB. */
const MAX_TOTAL_BYTES = 96 * 1024 * 1024;
/** The file on disk is base64 (about 4/3 larger); refuse to read anything absurdly big. */
const MAX_FILE_BYTES = 160 * 1024 * 1024;

/** The engine's accepted print-bed sides, shared with the fit check. */
const BED_MIN_MM = 50;
const BED_MAX_MM = 2000;

/** The widest a `Date` and `Intl.DateTimeFormat` can express, in milliseconds. */
const MAX_TIMESTAMP_MS = 8.64e15;

/** The exact record IDs in the whole export, repeated in each independently restorable part. */
interface BackupIdentities {
  projects: string[];
  profiles: string[];
}

/** A decoded, validated backup, ready to preview and restore. */
export interface WorkspaceBackup {
  version: 1;
  projects: SavedProject[];
  profiles: PrinterProfile[];
  preferences: Omit<WorkspacePreferences, 'lastProjectId'>;
  exportedIds?: BackupIdentities;
}

interface BackupPart extends WorkspaceSnapshot {
  exportedIds?: BackupIdentities;
}

export interface BackupPlan {
  parts: BackupPart[];
  excluded: Pick<SavedProject, 'id' | 'title'>[];
}

type EncodedProject = Omit<SavedProject, 'file'> & { file: string };

function envelope(
  { defaults, theme, defaultProfileId }: WorkspacePreferences,
  exportedIds: BackupIdentities | undefined,
  projects: EncodedProject[] = [],
  profiles: PrinterProfile[] = []
) {
  return {
    [BACKUP_FORMAT]: BACKUP_VERSION,
    projects,
    profiles,
    preferences: { defaults, theme, defaultProfileId },
    ...(exportedIds ? { exportedIds } : {})
  };
}

const jsonBytes = (value: unknown): number => new Blob([JSON.stringify(value)]).size;

const asObject = (value: unknown): Record<string, unknown> | undefined =>
  typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;

const asString = (value: unknown): string | undefined =>
  typeof value === 'string' ? value : undefined;

const asNumber = (value: unknown): number =>
  typeof value === 'number' && Number.isFinite(value) ? value : NaN;

function asArray(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw new WorkspaceError('invalid');
  return value;
}

function decodeIds(raw: unknown, records: unknown[]): string[] {
  const ids = new Set<string>();
  for (const value of asArray(raw)) {
    const id = asString(value);
    if (!id || ids.has(id)) throw new WorkspaceError('invalid');
    ids.add(id);
  }
  if (records.some((record) => !ids.has(asString(asObject(record)?.id) ?? ''))) {
    throw new WorkspaceError('invalid');
  }
  return [...ids];
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

function base64ToBytes(text: string): Uint8Array<ArrayBuffer> {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** The SHA-256 fingerprint of a file, recomputed from its actual bytes. */
async function digest(file: Blob): Promise<string> {
  const hash = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
  return Array.from(new Uint8Array(hash), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function checkBed(mm: number): void {
  if (Number.isNaN(mm) || mm < BED_MIN_MM || mm > BED_MAX_MM) throw new WorkspaceError('invalid');
}

const VIEWS = ['picture', 'chart', 'both', 'stencil', 'result', 'bed'] as const;
const HEX = /^#[0-9a-f]{6}$/i;

function decodeView(value: unknown): SavedProject['view'] {
  const obj = asObject(value);
  if (!obj) throw new WorkspaceError('invalid');
  if (obj.view === undefined || !VIEWS.includes(obj.view as (typeof VIEWS)[number])) {
    throw new WorkspaceError('invalid');
  }
  if (obj.plateView !== 'top' && obj.plateView !== '3d') throw new WorkspaceError('invalid');
  const selectedColour = obj.selectedColour === null ? null : asString(obj.selectedColour);
  if (selectedColour === undefined || (selectedColour !== null && !HEX.test(selectedColour))) {
    throw new WorkspaceError('invalid');
  }
  if ((obj.bedPart !== null && obj.bedPart !== 'frame') || typeof obj.comparePaint !== 'boolean')
    throw new WorkspaceError('invalid');
  return {
    view: obj.view as SavedProject['view']['view'],
    plateView: obj.plateView,
    selectedColour,
    bedPart: obj.bedPart,
    comparePaint: obj.comparePaint
  };
}

function decodeOrigin(value: unknown): Origin | undefined {
  if (value === undefined) return undefined;
  const obj = asObject(value);
  if (!obj) throw new WorkspaceError('invalid');
  const id = asString(obj.id);
  const name = asString(obj.name);
  const importedAs = asString(obj.importedAs);
  const sourceId = asString(obj.sourceId);
  if (!id || name === undefined || importedAs === undefined) throw new WorkspaceError('invalid');
  if (obj.sourceId !== undefined && !sourceId) throw new WorkspaceError('invalid');
  return { id, name, importedAs, ...(sourceId ? { sourceId } : {}) };
}

async function decodeProjects(
  raw: unknown[],
  profileIds: ReadonlySet<string>,
  validateProject: (file: Blob) => Promise<ProjectMode | null>
): Promise<SavedProject[]> {
  // First pass: shape, identity, sizes and timestamps, before any engine work.
  const seen = new Set<string>();
  const pending: Omit<SavedProject, 'digest'>[] = [];
  let total = 0;
  for (const item of raw) {
    const obj = asObject(item);
    if (!obj) throw new WorkspaceError('invalid');
    const id = asString(obj.id);
    const title = asString(obj.title);
    const mode = asString(obj.mode);
    const fileText = asString(obj.file);
    if (
      !id ||
      !title?.trim() ||
      (mode !== 'tiles' && mode !== 'freeform') ||
      fileText === undefined
    ) {
      throw new WorkspaceError('invalid');
    }
    if (seen.has(id)) throw new WorkspaceError('invalid');
    seen.add(id);
    const editedAt = asNumber(obj.editedAt);
    const revision = asNumber(obj.revision);
    if (Number.isNaN(editedAt) || Number.isNaN(revision) || Math.abs(editedAt) > MAX_TIMESTAMP_MS) {
      throw new WorkspaceError('invalid');
    }
    const view = decodeView(obj.view);
    const profileId = obj.profileId === null ? null : asString(obj.profileId);
    if (profileId === undefined || (profileId !== null && !profileIds.has(profileId)))
      throw new WorkspaceError('invalid');
    // Reject oversized base64 before allocating a second copy of the data.
    const remaining = Math.min(MAX_PROJECT_BYTES, MAX_TOTAL_BYTES - total);
    if (fileText.length > 4 * Math.ceil(remaining / 3)) throw new WorkspaceError('too-large');
    let bytes: Uint8Array<ArrayBuffer>;
    try {
      bytes = base64ToBytes(fileText);
    } catch {
      throw new WorkspaceError('invalid');
    }
    if (bytes.length > MAX_PROJECT_BYTES) throw new WorkspaceError('too-large');
    total += bytes.length;
    if (total > MAX_TOTAL_BYTES) throw new WorkspaceError('too-large');
    const origin = decodeOrigin(obj.origin);
    pending.push({
      id,
      revision,
      editedAt,
      title,
      mode,
      view,
      profileId,
      file: new Blob([bytes], { type: 'application/json' }),
      ...(origin ? { origin } : {})
    });
  }
  // Second pass: engine validation and fingerprinting, once everything is known to fit.
  const projects: SavedProject[] = [];
  for (const record of pending) {
    let validated: ProjectMode | null;
    try {
      validated = await validateProject(record.file);
    } catch {
      throw new WorkspaceError('validation-unavailable');
    }
    if (validated !== record.mode) throw new WorkspaceError('invalid');
    projects.push({ ...record, digest: await digest(record.file) });
  }
  return projects;
}

function decodeProfiles(raw: unknown[]): PrinterProfile[] {
  const seen = new Set<string>();
  const profiles: PrinterProfile[] = [];
  for (const item of raw) {
    const obj = asObject(item);
    if (!obj) throw new WorkspaceError('invalid');
    const id = asString(obj.id);
    const name = asString(obj.name);
    if (!id || !name?.trim()) throw new WorkspaceError('invalid');
    if (seen.has(id)) throw new WorkspaceError('invalid');
    seen.add(id);
    const revision = asNumber(obj.revision);
    if (Number.isNaN(revision)) throw new WorkspaceError('invalid');
    const widthMm = asNumber(obj.widthMm);
    const depthMm = asNumber(obj.depthMm);
    checkBed(widthMm);
    checkBed(depthMm);
    const optional = (key: string): number | undefined => {
      if (obj[key] === undefined) return undefined;
      const value = asNumber(obj[key]);
      if (Number.isNaN(value) || value <= 0) throw new WorkspaceError('invalid');
      return value;
    };
    const lineWidthMm = optional('lineWidthMm');
    const layerHeightMm = optional('layerHeightMm');
    const nozzleMm = optional('nozzleMm');
    const origin = decodeOrigin(obj.origin);
    profiles.push({
      id,
      revision,
      name,
      widthMm,
      depthMm,
      ...(lineWidthMm === undefined ? {} : { lineWidthMm }),
      ...(layerHeightMm === undefined ? {} : { layerHeightMm }),
      ...(nozzleMm === undefined ? {} : { nozzleMm }),
      ...(origin ? { origin } : {})
    });
  }
  return profiles;
}

function decodePreferences(raw: unknown): Omit<WorkspacePreferences, 'lastProjectId'> {
  const obj = asObject(raw);
  if (!obj) throw new WorkspaceError('invalid');
  const theme = asString(obj.theme);
  if (theme !== 'auto' && theme !== 'light' && theme !== 'dark')
    throw new WorkspaceError('invalid');
  const defaultProfileId = obj.defaultProfileId === null ? null : asString(obj.defaultProfileId);
  const defaults = asObject(obj.defaults);
  if (defaultProfileId === undefined || !defaults) throw new WorkspaceError('invalid');
  return { defaults: sanitise(defaults), theme, defaultProfileId };
}

/**
 * The name a record was imported under, or its current name once the person renamed it.
 * Only when the current name still equals `origin.importedAs` is `origin.name` a fair
 * comparison for the source it came from.
 */
function comparableName<T extends { origin?: Origin }>(
  record: T,
  nameOf: (record: T) => string
): string {
  const origin = record.origin;
  if (origin === undefined) return nameOf(record);
  return nameOf(record) === origin.importedAs ? origin.name : nameOf(record);
}

/** A name distinct from every name already taken, comparing without case. */
function distinctName(base: string, taken: Set<string>): string {
  const key = base.toLocaleLowerCase();
  if (!taken.has(key)) {
    taken.add(key);
    return base;
  }
  for (let n = 1; ; n++) {
    const candidate = `${base}${text.workspace.copySuffix(n)}`;
    const candidateKey = candidate.toLocaleLowerCase();
    if (!taken.has(candidateKey)) {
      taken.add(candidateKey);
      return candidate;
    }
  }
}

function sameView(a: SavedProject['view'], b: SavedProject['view']): boolean {
  return (
    a.view === b.view &&
    a.plateView === b.plateView &&
    a.selectedColour === b.selectedColour &&
    a.bedPart === b.bedPart &&
    a.comparePaint === b.comparePaint
  );
}

function sameProfile(a: PrinterProfile, b: PrinterProfile): boolean {
  return (
    a.widthMm === b.widthMm &&
    a.depthMm === b.depthMm &&
    a.lineWidthMm === b.lineWidthMm &&
    a.layerHeightMm === b.layerHeightMm &&
    a.nozzleMm === b.nozzleMm
  );
}

/**
 * How each source record compares to the workspace: skip an identical record, copy a
 * changed one under a fresh id, or add a new one. A common origin is not enough:
 * separate saved copies have their own ids, even in different backup files.
 */
function planRecords<T extends { id: string; origin?: Origin }>(
  sources: T[],
  existing: T[],
  exportedIds: string[] | undefined,
  same: (candidate: T, source: T) => boolean,
  nameOf: (record: T) => string,
  withName: (record: T, name: string) => T,
  remap: (record: T) => T
): RestoreEntry<T>[] {
  const taken = new Set(existing.map((record) => nameOf(record).toLocaleLowerCase()));
  const sourceIds = new Set(exportedIds ?? sources.map(({ id }) => id));
  return sources.map((source) => {
    const incoming = remap(source);
    const sourceName = comparableName(source, nameOf);
    // Reserve exact ids for their own rows, including rows in other parts. On older restores,
    // the original id is the only evidence of the exported id.
    const candidates = existing.filter(
      (record) =>
        record.id === source.id ||
        (!sourceIds.has(record.id) && (record.origin?.sourceId ?? record.origin?.id) === source.id)
    );
    const matches = candidates.filter(
      (record) => comparableName(record, nameOf) === sourceName && same(record, incoming)
    );
    const identical = matches.find((record) => record.id === source.id) ?? matches[0];
    if (identical) return { source, record: identical, action: 'skip' };

    const name = distinctName(nameOf(source), taken);
    const origin: Origin = {
      id: source.origin?.id ?? source.id,
      sourceId: source.id,
      name: sourceName,
      importedAs: name
    };
    const action = candidates.length ? 'copy' : 'add';
    const id = action === 'copy' ? crypto.randomUUID() : source.id;
    const record = withName({ ...incoming, id, revision: 0, origin }, name);
    return { source, record, action };
  });
}

/**
 * Splits one saved snapshot into files that can be imported independently. Every part
 * carries the default profile and any profiles needed by its projects.
 */
export function planBackups(snapshot: WorkspaceSnapshot): BackupPlan {
  const included: SavedProject[] = [];
  const excluded: BackupPlan['excluded'] = [];
  for (const project of snapshot.projects) {
    if (project.file.size > MAX_PROJECT_BYTES)
      excluded.push({ id: project.id, title: project.title });
    else included.push(project);
  }
  const exportedIds: BackupIdentities = {
    projects: included.map(({ id }) => id),
    profiles: snapshot.profiles.map(({ id }) => id)
  };
  const allProfiles = new Map(
    snapshot.profiles.map((profile) => [profile.id, { profile, bytes: jsonBytes(profile) + 1 }])
  );
  const defaultProfile = allProfiles.get(snapshot.preferences.defaultProfileId ?? '');
  const unassigned = new Set(allProfiles.keys());
  if (defaultProfile) unassigned.delete(defaultProfile.profile.id);

  const emptyBytes =
    jsonBytes(envelope(snapshot.preferences, exportedIds)) + (defaultProfile?.bytes ?? 0);
  let encodedBytes = emptyBytes;
  let projectBytes = 0;
  let projects: SavedProject[] = [];
  const profiles = new Map<string, PrinterProfile>();
  if (defaultProfile) profiles.set(defaultProfile.profile.id, defaultProfile.profile);
  const parts: BackupPart[] = [];

  function finishPart(): void {
    parts.push({ ...snapshot, projects, profiles: [...profiles.values()], exportedIds });
    projects = [];
    profiles.clear();
    if (defaultProfile) profiles.set(defaultProfile.profile.id, defaultProfile.profile);
    projectBytes = 0;
    encodedBytes = emptyBytes;
  }

  if (emptyBytes > MAX_FILE_BYTES) throw new WorkspaceError('too-large');
  for (const project of included) {
    const size = project.file.size;
    // Base64 uses four ASCII bytes per three input bytes. Reserve one byte for the comma.
    const bytes = jsonBytes({ ...project, file: '' }) + 4 * Math.ceil(size / 3) + 1;
    const linked = allProfiles.get(project.profileId ?? '');
    const profileBytes = () => (linked && !profiles.has(linked.profile.id) ? linked.bytes : 0);
    if (
      projects.length === MAX_ITEMS ||
      projectBytes + size > MAX_TOTAL_BYTES ||
      (profileBytes() > 0 && profiles.size === MAX_ITEMS) ||
      encodedBytes + bytes + profileBytes() > MAX_FILE_BYTES
    ) {
      finishPart();
    }
    if (encodedBytes + bytes + profileBytes() > MAX_FILE_BYTES)
      throw new WorkspaceError('too-large');
    projects.push(project);
    projectBytes += size;
    encodedBytes += bytes + profileBytes();
    if (linked) {
      profiles.set(linked.profile.id, linked.profile);
      unassigned.delete(linked.profile.id);
    }
  }
  for (const { profile, bytes } of allProfiles.values()) {
    if (!unassigned.has(profile.id)) continue;
    if (profiles.size === MAX_ITEMS || encodedBytes + bytes > MAX_FILE_BYTES) finishPart();
    if (encodedBytes + bytes > MAX_FILE_BYTES) throw new WorkspaceError('too-large');
    profiles.set(profile.id, profile);
    encodedBytes += bytes;
  }
  finishPart();
  return { parts, excluded };
}

export async function encodeBackup(snapshot: BackupPart): Promise<Blob> {
  if (snapshot.projects.length > MAX_ITEMS || snapshot.profiles.length > MAX_ITEMS) {
    throw new WorkspaceError('too-large');
  }
  const total = snapshot.projects.reduce((bytes, project) => bytes + project.file.size, 0);
  if (
    total > MAX_TOTAL_BYTES ||
    snapshot.projects.some(({ file }) => file.size > MAX_PROJECT_BYTES)
  )
    throw new WorkspaceError('too-large');
  const projects: EncodedProject[] = [];
  for (const project of snapshot.projects) {
    projects.push({
      ...project,
      file: bytesToBase64(new Uint8Array(await project.file.arrayBuffer()))
    });
  }
  const backup = new Blob(
    [
      JSON.stringify(
        envelope(snapshot.preferences, snapshot.exportedIds, projects, snapshot.profiles)
      )
    ],
    {
      type: 'application/json'
    }
  );
  if (backup.size > MAX_FILE_BYTES) throw new WorkspaceError('too-large');
  return backup;
}

export async function decodeBackup(
  file: Blob,
  validateProject: (file: Blob) => Promise<ProjectMode | null>
): Promise<WorkspaceBackup> {
  if (file.size > MAX_FILE_BYTES) throw new WorkspaceError('too-large');
  let raw: unknown;
  try {
    raw = JSON.parse(await file.text());
  } catch {
    throw new WorkspaceError('invalid');
  }
  const root = asObject(raw);
  if (!root || !(BACKUP_FORMAT in root)) throw new WorkspaceError('invalid');
  if (root[BACKUP_FORMAT] !== BACKUP_VERSION) throw new WorkspaceError('unsupported');
  const projectItems = asArray(root.projects);
  const profileItems = asArray(root.profiles);
  if (projectItems.length > MAX_ITEMS || profileItems.length > MAX_ITEMS) {
    throw new WorkspaceError('too-large');
  }
  let exportedIds: BackupIdentities | undefined;
  if (root.exportedIds !== undefined) {
    const ids = asObject(root.exportedIds);
    if (!ids) throw new WorkspaceError('invalid');
    exportedIds = {
      projects: decodeIds(ids.projects, projectItems),
      profiles: decodeIds(ids.profiles, profileItems)
    };
  }
  const profiles = decodeProfiles(profileItems);
  const preferences = decodePreferences(root.preferences);
  const profileIds = new Set(profiles.map(({ id }) => id));
  if (preferences.defaultProfileId !== null && !profileIds.has(preferences.defaultProfileId)) {
    throw new WorkspaceError('invalid');
  }
  const projects = await decodeProjects(projectItems, profileIds, validateProject);
  return {
    version: BACKUP_VERSION,
    projects,
    profiles,
    preferences,
    ...(exportedIds ? { exportedIds } : {})
  };
}

export function previewRestore(backup: WorkspaceBackup, snapshot: WorkspaceSnapshot): RestorePlan {
  const profiles = planRecords(
    backup.profiles,
    snapshot.profiles,
    backup.exportedIds?.profiles,
    sameProfile,
    (profile) => profile.name,
    (profile, name) => ({ ...profile, name }),
    (profile) => profile
  );
  const profileMap = new Map<string, string>();
  for (const entry of profiles) profileMap.set(entry.source.id, entry.record.id);

  const remap = (record: SavedProject): SavedProject => ({
    ...record,
    profileId: record.profileId ? (profileMap.get(record.profileId) ?? null) : null
  });

  const projects = planRecords(
    backup.projects,
    snapshot.projects,
    backup.exportedIds?.projects,
    (candidate, source) =>
      sameView(candidate.view, source.view) &&
      candidate.digest === source.digest &&
      candidate.profileId === source.profileId,
    (project) => project.title,
    (project, title) => ({ ...project, title }),
    remap
  );

  return {
    revision: snapshot.revision,
    projects,
    profiles,
    preferences: {
      defaults: backup.preferences.defaults,
      theme: backup.preferences.theme,
      defaultProfileId: backup.preferences.defaultProfileId
        ? (profileMap.get(backup.preferences.defaultProfileId) ?? null)
        : null
    }
  };
}
