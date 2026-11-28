/**
 * Portable workspace backups and the restore preview built from them. Each file is a
 * versioned JSON envelope carrying each project's exact `.stencilator` bytes as base64.
 * Decoding validates everything before any write: size, envelope version, unique ids,
 * finite timestamps, profile dimensions within the engine's bed range, view shape, project
 * mode against the engine's own validation, saved defaults against the engine's settings
 * ranges, restored metadata against the backup file budget, and project-to-profile links.
 * The restore preview is deterministic: it skips, adds or copies by identity and saved
 * content, never by name alone, and remaps linked profile ids while the applied bed stays
 * inside the preserved project bytes.
 */
import type { ProjectMode } from '../engine/client';
import { sanitise, type Settings } from './settings';
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
/** The longest record, origin or exported id, in UTF-16 code units. */
const MAX_ID_LENGTH = 256;

/** A fresh id from `crypto.randomUUID()`, in UTF-16 code units. */
const RESTORED_ID_LENGTH = 36;
/** Enough for a generated copy suffix, including locale grouping and a large number. */
const COPY_SUFFIX_HEADROOM = 64;
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

/** A saved record left out of an export because it cannot fit an importable file. */
interface ExcludedRecord {
  kind: 'project' | 'profile';
  id: string;
  title: string;
}

export interface BackupPlan {
  parts: BackupPart[];
  excluded: ExcludedRecord[];
}

type EncodedProject = Omit<SavedProject, 'file'> & { file: string };

function envelope(
  { defaults, theme, defaultProfileId }: Omit<WorkspacePreferences, 'lastProjectId'>,
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

/** Printable ASCII without a quote or backslash, which JSON carries one byte per character. */
const PLAIN_ASCII = /^[\x20\x21\x23-\x5b\x5d-\x7e]*$/;

/** The bytes a JSON string adds, without materialising a second copy of a large name. */
function jsonStringBytes(value: string): number {
  if (PLAIN_ASCII.test(value)) return value.length + 2;
  let bytes = 2;
  for (let i = 0; i < value.length;) {
    const code = value.codePointAt(i) ?? 0;
    if (code === 0x22 || code === 0x5c) bytes += 2;
    else if (code === 0x08 || code === 0x09 || code === 0x0a || code === 0x0c || code === 0x0d) {
      bytes += 2;
    } else if (code < 0x20 || (code >= 0xd800 && code <= 0xdfff)) bytes += 6;
    else bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    i += code > 0xffff ? 2 : 1;
  }
  return bytes;
}

/** A string's JSON content bytes, excluding the quotes the blank field already carries. */
const contentBytes = (value: string): number => jsonStringBytes(value) - 2;

/**
 * The bytes a saved record adds to a backup file: its JSON, the comma after it, and a
 * project's base64 file. A restored record keeps its saved name in the record itself and
 * in both origin fields, so each name is measured once rather than materialising a string
 * several times the size of the original.
 */
function backupRecordBytes(record: SavedProject | PrinterProfile): number {
  const base: Record<string, unknown> = { ...record };
  let name: string;
  let fileBytes = 0;
  if ('file' in record) {
    name = record.title;
    base.file = '';
    base.title = '';
    // Base64 uses four ASCII bytes per three input bytes, and the comma follows the record.
    fileBytes = 4 * Math.ceil(record.file.size / 3);
  } else {
    name = record.name;
    base.name = '';
  }
  const names = [name, record.origin?.name ?? '', record.origin?.importedAs ?? ''];
  if (record.origin) base.origin = { ...record.origin, name: '', importedAs: '' };
  const encodedNames = names.reduce((sum, value) => (value ? sum + contentBytes(value) : sum), 0);
  return jsonBytes(base) + encodedNames + fileBytes + 1;
}

/**
 * The bytes a record needs after a restore gives it an origin, a possibly suffixed name in
 * both name fields and, for a copy, a fresh id. This is at least the saved name's own
 * length, so an incoming backup cannot grow a record beyond what it exported from.
 */
function futureRecordBytes(record: SavedProject | PrinterProfile): number {
  const base: Record<string, unknown> = { ...record };
  let name: string;
  let fileBytes = 0;
  if ('file' in record) {
    name = record.title;
    base.file = '';
    base.title = '';
    fileBytes = 4 * Math.ceil(record.file.size / 3);
  } else {
    name = record.name;
    base.name = '';
  }
  base.id = '';
  base.origin = { id: '', sourceId: '', name: '', importedAs: '' };
  const allocated = contentBytes(name) + COPY_SUFFIX_HEADROOM;
  const originId = record.origin?.id ?? record.id;
  return (
    jsonBytes(base) +
    fileBytes +
    1 +
    allocated +
    contentBytes(name) +
    allocated +
    Math.max(contentBytes(record.id), RESTORED_ID_LENGTH) +
    contentBytes(originId) +
    contentBytes(record.id)
  );
}

/** The bytes to reserve for a record both as exported now and as restored next time. */
function reservedRecordBytes(record: SavedProject | PrinterProfile): number {
  return Math.max(backupRecordBytes(record), futureRecordBytes(record));
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

function decodeIds(raw: unknown, records: unknown[]): string[] {
  const ids = new Set<string>();
  for (const value of asArray(raw)) {
    const id = asString(value);
    if (!id || id.length > MAX_ID_LENGTH || ids.has(id)) throw new WorkspaceError('invalid');
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

type NumberField = {
  [K in keyof Settings]: Settings[K] extends number ? K : never;
}[keyof Settings];
type Range = readonly [number, number];

/**
 * The engine's accepted settings ranges, which a saved default must meet in every mode.
 * The interface leaves a live out-of-range entry for the engine to explain; a restored
 * default offered for new projects has no such moment. As storage's `sanitise` does for a
 * missing or malformed value, an unusable range takes the engine's default rather than
 * rejecting a backup with otherwise usable records, and every valid explicit value is kept.
 *
 * A default seeds projects in any mode and presets may switch sizing or mode, so every
 * field is checked even while its mode is inactive. This is deliberately stricter than a
 * single engine check, which ignores inactive fields: a default is only useful if it stays
 * usable after such a switch. Paint merges and skipped paints are valid lists, not ranges,
 * and are kept exactly as saved, without deduplication or truncation.
 */
const DEFAULT_RANGES: Partial<Record<NumberField, Range>> = {
  canvasWidthMm: [1, 10_000],
  canvasHeightMm: [1, 10_000],
  tileMm: [0.1, 1_000],
  lineMm: [0, 20],
  artWidthMm: [1, 10_000],
  artHeightMm: [1, 10_000],
  cellMm: [0.05, 0.5],
  minPatchMm2: [0, 100],
  marginMm: [2, 100],
  minWebMm: [0.1, 5],
  minOpeningMm: [0.1, 5],
  bridgeWidthMm: [0.6, 5],
  connectorWidthMm: [0.6, 5],
  connectorClearanceMm: [0.4, 30],
  beamMm: [0.4, 5],
  warnSpanMm: [1, 200],
  thicknessMm: [0.2, 20],
  bedWidthMm: [BED_MIN_MM, BED_MAX_MM],
  bedDepthMm: [BED_MIN_MM, BED_MAX_MM]
};

/** Ranges checked after the engine's rounding of a whole number. */
const DEFAULT_WHOLE_RANGES: Partial<Record<NumberField, Range>> = {
  colours: [1, 256],
  dpi: [72, 2_400]
};

/** Settings that accept zero, such as no chamfer, or a value within a range. */
const DEFAULT_ZERO_OR_RANGES: Partial<Record<NumberField, Range>> = {
  chamferMm: [0.1, 2],
  overlapMm: [0.1, 2]
};

const inRange = (value: number, [min, max]: Range): boolean =>
  Number.isFinite(value) && value >= min && value <= max;

/** Whole micrometres, rounding as the engine does, for its thickness comparisons. */
function micrometres(mm: number): number {
  const scaled = mm * 1_000;
  const lower = Math.floor(scaled);
  const fraction = scaled - lower;
  if (fraction < 0.5) return lower;
  if (fraction > 0.5) return lower + 1;
  return lower % 2 === 0 ? lower : lower + 1;
}

/** Saved defaults with unusable values replaced, so new projects can always be saved. */
function checkedDefaults(raw: unknown): Settings {
  const settings = sanitise(raw);
  const fallback = sanitise(undefined);
  for (const [key, range] of Object.entries(DEFAULT_RANGES) as [NumberField, Range][]) {
    if (!inRange(settings[key], range)) settings[key] = fallback[key];
  }
  for (const [key, range] of Object.entries(DEFAULT_WHOLE_RANGES) as [NumberField, Range][]) {
    if (!inRange(Math.round(settings[key]), range)) settings[key] = fallback[key];
  }
  for (const [key, range] of Object.entries(DEFAULT_ZERO_OR_RANGES) as [NumberField, Range][]) {
    if (settings[key] !== 0 && !inRange(settings[key], range)) settings[key] = fallback[key];
  }
  // A raised connector needs 0.2 mm of clearance above the plate itself. Compare whole
  // micrometres, as the engine does, so a value it accepts is never changed by rounding.
  const thickness = micrometres(settings.thicknessMm);
  if (micrometres(settings.connectorClearanceMm) - thickness < 200) {
    settings.connectorClearanceMm = Math.max(
      fallback.connectorClearanceMm,
      (thickness + 200) / 1_000
    );
  }
  // A chamfer must leave 0.2 mm of plate at the work face.
  if (settings.chamferMm !== 0 && thickness - micrometres(settings.chamferMm) < 200) {
    settings.chamferMm = fallback.chamferMm;
  }
  return settings;
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
  if (!id || id.length > MAX_ID_LENGTH || name === undefined || importedAs === undefined) {
    throw new WorkspaceError('invalid');
  }
  if (obj.sourceId !== undefined && (!sourceId || sourceId.length > MAX_ID_LENGTH)) {
    throw new WorkspaceError('invalid');
  }
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
      id.length > MAX_ID_LENGTH ||
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
    if (
      profileId === undefined ||
      (profileId !== null && (profileId.length > MAX_ID_LENGTH || !profileIds.has(profileId)))
    ) {
      throw new WorkspaceError('invalid');
    }
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
    if (!id || id.length > MAX_ID_LENGTH || !name?.trim()) throw new WorkspaceError('invalid');
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
  if (
    defaultProfileId === undefined ||
    (defaultProfileId !== null && defaultProfileId.length > MAX_ID_LENGTH) ||
    !defaults
  ) {
    throw new WorkspaceError('invalid');
  }
  return { defaults: checkedDefaults(defaults), theme, defaultProfileId };
}

/** The saved name, allowing an unchanged name allocated to avoid a clash during restore. */
function matchesSavedName<T extends { id: string; origin?: Origin }>(
  candidate: T,
  source: T,
  nameOf: (record: T) => string
): boolean {
  const savedName = nameOf(source);
  const currentName = nameOf(candidate);
  if (currentName === savedName) return true;

  const { origin } = candidate;
  if (
    origin?.importedAs !== currentName ||
    (origin.sourceId !== undefined && origin.sourceId !== source.id) ||
    !currentName.startsWith(savedName)
  ) {
    return false;
  }

  // Older restores of copies could store the ancestor's name. In either case, the
  // allocated name must be the incoming saved name followed by one restore suffix.
  const suffix = currentName.slice(savedName.length);
  const number = Number(suffix.replace(/\D/g, '') || '1');
  if (number < 1 || !Number.isSafeInteger(number) || suffix !== text.workspace.copySuffix(number))
    return false;
  const direct = (origin.sourceId ?? origin.id) === source.id;
  const earlier = source.origin;
  return (
    (direct && origin.name === savedName) ||
    (earlier?.id === origin.id && earlier.name === origin.name && earlier.importedAs === savedName)
  );
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
    const sourceName = nameOf(source);
    // Reserve exact ids for their own rows, including rows in other parts. On older restores,
    // the original id is the only evidence of the exported id.
    const candidates = existing.filter(
      (record) =>
        record.id === source.id ||
        (!sourceIds.has(record.id) && (record.origin?.sourceId ?? record.origin?.id) === source.id)
    );
    const matches = candidates.filter(
      (record) => matchesSavedName(record, source, nameOf) && same(record, incoming)
    );
    const identical = matches.find((record) => record.id === source.id) ?? matches[0];
    if (identical) return { source, record: identical, action: 'skip' };

    const name = distinctName(sourceName, taken);
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
  const excluded: ExcludedRecord[] = [];
  let included: SavedProject[] = [];
  for (const project of snapshot.projects) {
    if (project.file.size > MAX_PROJECT_BYTES || reservedRecordBytes(project) > MAX_FILE_BYTES) {
      excluded.push({ kind: 'project', id: project.id, title: project.title });
    } else {
      included.push(project);
    }
  }

  const allProfiles = new Map<string, { profile: PrinterProfile; bytes: number }>();
  for (const profile of snapshot.profiles) {
    const bytes = reservedRecordBytes(profile);
    if (bytes > MAX_FILE_BYTES) {
      excluded.push({ kind: 'profile', id: profile.id, title: profile.name });
    } else {
      allProfiles.set(profile.id, { profile, bytes });
    }
  }

  let defaultProfile = allProfiles.get(snapshot.preferences.defaultProfileId ?? '');
  const preferences = (): WorkspacePreferences => ({
    ...snapshot.preferences,
    defaultProfileId: defaultProfile ? snapshot.preferences.defaultProfileId : null
  });
  const identities = (): BackupIdentities => ({
    projects: included.map(({ id }) => id),
    profiles: [...allProfiles.keys()]
  });
  const envelopeBytes = (): number => jsonBytes(envelope(preferences(), identities()));

  /** Removes projects whose linked profile was left out, so no part carries a broken link. */
  const dropBrokenLinks = (): void => {
    included = included.filter((project) => {
      if (project.profileId === null || allProfiles.has(project.profileId)) return true;
      excluded.push({ kind: 'project', id: project.id, title: project.title });
      return false;
    });
  };
  dropBrokenLinks();

  // A default profile that cannot share a fresh part with the envelope is left out; every
  // project linked to it follows, while the rest of the workspace still exports. A part
  // cannot refer to a printer it does not carry, so its preferences carry no default and
  // the excluded list names the one left out.
  if (defaultProfile && envelopeBytes() + defaultProfile.bytes > MAX_FILE_BYTES) {
    excluded.push({
      kind: 'profile',
      id: defaultProfile.profile.id,
      title: defaultProfile.profile.name
    });
    allProfiles.delete(defaultProfile.profile.id);
    defaultProfile = undefined;
    dropBrokenLinks();
  }
  if (envelopeBytes() > MAX_FILE_BYTES) throw new WorkspaceError('too-large');

  // A profile that cannot share a fresh part with the envelope and default printer is
  // reported rather than aborting the export of everything else.
  for (const [id, { profile, bytes }] of allProfiles) {
    if (id === defaultProfile?.profile.id) continue;
    if (envelopeBytes() + (defaultProfile?.bytes ?? 0) + bytes > MAX_FILE_BYTES) {
      excluded.push({ kind: 'profile', id, title: profile.name });
      allProfiles.delete(id);
    }
  }
  dropBrokenLinks();

  // Likewise for a project whose own metadata and file cannot fit a fresh part.
  included = included.filter((project) => {
    const linked = allProfiles.get(project.profileId ?? '');
    const extra = linked && linked.profile.id !== defaultProfile?.profile.id ? linked.bytes : 0;
    const fits =
      envelopeBytes() + (defaultProfile?.bytes ?? 0) + reservedRecordBytes(project) + extra;
    if (fits <= MAX_FILE_BYTES) return true;
    excluded.push({ kind: 'project', id: project.id, title: project.title });
    return false;
  });

  const exportedIds: BackupIdentities = identities();
  const emptyBytes = envelopeBytes() + (defaultProfile?.bytes ?? 0);
  if (emptyBytes > MAX_FILE_BYTES) throw new WorkspaceError('too-large');

  const unassigned = new Set(allProfiles.keys());
  if (defaultProfile) unassigned.delete(defaultProfile.profile.id);
  let encodedBytes = emptyBytes;
  let projectBytes = 0;
  let projects: SavedProject[] = [];
  const profiles = new Map<string, PrinterProfile>();
  if (defaultProfile) profiles.set(defaultProfile.profile.id, defaultProfile.profile);
  const parts: BackupPart[] = [];

  function finishPart(): void {
    parts.push({
      ...snapshot,
      preferences: preferences(),
      projects,
      profiles: [...profiles.values()],
      exportedIds
    });
    projects = [];
    profiles.clear();
    if (defaultProfile) profiles.set(defaultProfile.profile.id, defaultProfile.profile);
    projectBytes = 0;
    encodedBytes = emptyBytes;
  }

  for (const project of included) {
    const size = project.file.size;
    const bytes = reservedRecordBytes(project);
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
  // Preferences, including paint merges and skipped paints, travel in every backup part.
  // They are valid lists and are kept as saved, but a block that cannot fit a file on its
  // own would make every later export impossible, so refuse it before anything is stored.
  const identities = exportedIds ?? {
    projects: projects.map(({ id }) => id),
    profiles: profiles.map(({ id }) => id)
  };
  if (jsonBytes(envelope(preferences, identities)) > MAX_FILE_BYTES) {
    throw new WorkspaceError('too-large');
  }
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

  const preferences = {
    // Decoding already checked these; a hand-built backup cannot bypass the same guard.
    defaults: checkedDefaults(backup.preferences.defaults),
    theme: backup.preferences.theme,
    defaultProfileId: backup.preferences.defaultProfileId
      ? (profileMap.get(backup.preferences.defaultProfileId) ?? null)
      : null
  };

  // Every backup part repeats the whole id set and one preferences block. Check the
  // workspace after this part is added with either preference choice, before any write.
  const projectIds = new Set(snapshot.projects.map(({ id }) => id));
  const profileIds = new Set(snapshot.profiles.map(({ id }) => id));
  for (const { action, record } of profiles) {
    if (action !== 'skip') profileIds.add(record.id);
  }
  for (const { action, record } of projects) {
    if (action !== 'skip') projectIds.add(record.id);
  }
  const identities = { projects: [...projectIds], profiles: [...profileIds] };
  const envelopeBytes = Math.max(
    jsonBytes(envelope(snapshot.preferences, identities)),
    jsonBytes(envelope(preferences, identities))
  );
  if (envelopeBytes > MAX_FILE_BYTES) throw new WorkspaceError('too-large');

  // A restored record adds its saved name to both origin fields and might need a copy id.
  // Reserve that future form too, so the next export need not drop what was restored.
  for (const { action, record } of [...profiles, ...projects]) {
    if (action !== 'skip' && envelopeBytes + reservedRecordBytes(record) > MAX_FILE_BYTES) {
      throw new WorkspaceError('too-large');
    }
  }

  return {
    revision: snapshot.revision,
    projects,
    profiles,
    preferences
  };
}
