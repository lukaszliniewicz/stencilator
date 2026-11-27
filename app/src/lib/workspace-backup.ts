/**
 * One portable workspace backup and the restore preview built from it. The file is a
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

/** A decoded, validated backup, ready to preview and restore. */
export interface WorkspaceBackup {
  version: 1;
  projects: SavedProject[];
  profiles: PrinterProfile[];
  preferences: Omit<WorkspacePreferences, 'lastProjectId'>;
}

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
  const id = obj === undefined ? undefined : asString(obj.id);
  const name = obj === undefined ? undefined : asString(obj.name);
  const importedAs = obj === undefined ? undefined : asString(obj.importedAs);
  if (!id || name === undefined || importedAs === undefined) throw new WorkspaceError('invalid');
  return { id, name, importedAs };
}

async function decodeProjects(
  raw: unknown[],
  profileIds: ReadonlySet<string>,
  validateProject: (file: Blob) => Promise<ProjectMode>
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
    let validated: ProjectMode;
    try {
      validated = await validateProject(record.file);
    } catch {
      throw new WorkspaceError('invalid');
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

/** A record's identity, stable across imports: its origin when it has one, else its id. */
function stableId(record: { id: string; origin?: Origin }): string {
  return record.origin?.id ?? record.id;
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
 * changed one under a fresh id, or add a new one. Identity is the exact id or a common
 * origin; among identical peers the exact id is preferred.
 */
function planRecords<T extends { id: string; origin?: Origin }>(
  sources: T[],
  existing: T[],
  same: (candidate: T, source: T) => boolean,
  nameOf: (record: T) => string,
  withName: (record: T, name: string) => T,
  remap: (record: T) => T
): RestoreEntry<T>[] {
  const taken = new Set(existing.map((record) => nameOf(record).toLocaleLowerCase()));
  return sources.map((source) => {
    const stable = stableId(source);
    const incoming = remap(source);
    const sourceName = comparableName(source, nameOf);
    const candidates = existing.filter(
      (record) => record.id === source.id || stableId(record) === stable
    );
    const matches = candidates.filter(
      (record) => comparableName(record, nameOf) === sourceName && same(record, incoming)
    );
    const identical = matches.find((record) => record.id === source.id) ?? matches[0];
    if (identical) return { source, record: identical, action: 'skip' };

    const name = distinctName(nameOf(source), taken);
    const origin: Origin = { id: stable, name: sourceName, importedAs: name };
    const action = candidates.length ? 'copy' : 'add';
    const id = action === 'copy' ? crypto.randomUUID() : source.id;
    const record = withName({ ...incoming, id, revision: 0, origin }, name);
    return { source, record, action };
  });
}

export async function encodeBackup(snapshot: WorkspaceSnapshot): Promise<Blob> {
  if (snapshot.projects.length > MAX_ITEMS || snapshot.profiles.length > MAX_ITEMS) {
    throw new WorkspaceError('too-large');
  }
  const total = snapshot.projects.reduce((bytes, project) => bytes + project.file.size, 0);
  if (
    total > MAX_TOTAL_BYTES ||
    snapshot.projects.some(({ file }) => file.size > MAX_PROJECT_BYTES)
  )
    throw new WorkspaceError('too-large');
  const projects: (Omit<SavedProject, 'file'> & { file: string })[] = [];
  for (const project of snapshot.projects) {
    projects.push({
      ...project,
      file: bytesToBase64(new Uint8Array(await project.file.arrayBuffer()))
    });
  }
  const preferences: Omit<WorkspacePreferences, 'lastProjectId'> = {
    defaults: snapshot.preferences.defaults,
    theme: snapshot.preferences.theme,
    defaultProfileId: snapshot.preferences.defaultProfileId
  };
  const envelope = {
    [BACKUP_FORMAT]: BACKUP_VERSION,
    projects,
    profiles: snapshot.profiles,
    preferences
  };
  return new Blob([JSON.stringify(envelope)], { type: 'application/json' });
}

export async function decodeBackup(
  file: Blob,
  validateProject: (file: Blob) => Promise<ProjectMode>
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
  const profiles = decodeProfiles(profileItems);
  const preferences = decodePreferences(root.preferences);
  const profileIds = new Set(profiles.map(({ id }) => id));
  if (preferences.defaultProfileId !== null && !profileIds.has(preferences.defaultProfileId)) {
    throw new WorkspaceError('invalid');
  }
  const projects = await decodeProjects(projectItems, profileIds, validateProject);
  return { version: BACKUP_VERSION, projects, profiles, preferences };
}

export function previewRestore(backup: WorkspaceBackup, snapshot: WorkspaceSnapshot): RestorePlan {
  const profiles = planRecords(
    backup.profiles,
    snapshot.profiles,
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
