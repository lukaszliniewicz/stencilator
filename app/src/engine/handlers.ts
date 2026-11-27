/**
 * Keeps the open project and its intermediate results in WebAssembly memory.
 * Both the worker and the tests run these same requests.
 */
import { hexToNumber } from '../lib/colour';
import { text } from '../lib/text';
import { ProjectSaveError } from './protocol';
import type {
  ArtworkEdits,
  ArtworkSource,
  Coat,
  FrameReport,
  FreeformStage,
  FreeformSummary,
  OpenedProject,
  PlateDrawing,
  PlateReport,
  ProjectSettings,
  RasterPreview,
  Replies,
  Request,
  SupportIssue
} from './protocol';
import type * as Wasm from './wasm/stencilator_wasm.js';

type Engine = Pick<
  typeof Wasm,
  'Artwork' | 'Fonts' | 'Project' | 'Source' | 'version' | 'recommendedMinWebMm' | 'bedFit'
>;

export interface Answer {
  value: Replies[Request['kind']];
  transfer: Transferable[];
}

interface Workspace {
  project: Wasm.Project;
  picture: Wasm.Source | undefined;
  artwork: Wasm.Artwork | undefined;
  labelled: { key: string; labels: Wasm.Labels } | undefined;
  fontRevision: number;
}

const file = (bytes: Uint8Array): Answer => ({ value: bytes, transfer: [bytes.buffer] });

function readProjectFile<T>(read: () => T): T {
  try {
    return read();
  } catch (error) {
    throw new Error(text.project.openFailed, { cause: error });
  }
}

function settingsOf(project: Wasm.Project): ProjectSettings {
  const settings = JSON.parse(project.settings) as ProjectSettings;
  const { mode, count, background } = settings.colours;
  return {
    ...settings,
    // A drawing cannot make a tile chart. Use the same mode on open and when
    // validating an unchanged import, even if its original bytes specify Tiles.
    mode: project.kind === 'picture' ? settings.mode : 'freeform',
    colours: { mode, count, background }
  };
}

function releaseArtwork(workspace: Workspace): void {
  workspace.labelled?.labels.free();
  workspace.labelled = undefined;
  workspace.artwork?.free();
  workspace.artwork = undefined;
}

function takePreview(preview: Wasm.RasterPreview): RasterPreview {
  const { width, height } = preview;
  return { width, height, rgba: preview.pixels() };
}

function applyActiveSettings(
  project: Wasm.Project,
  settings: ProjectSettings,
  use: 'preview' | 'save'
): void {
  const previous = settingsOf(project);
  const active =
    settings.mode === 'tiles'
      ? {
          ...settings,
          freeform: previous.freeform,
          stencils: {
            ...settings.stencils,
            // Thickness affects STL files, but an unfinished value should not block the chart.
            thickness_mm:
              use === 'preview' ? previous.stencils.thickness_mm : settings.stencils.thickness_mm,
            bed_mm: use === 'preview' ? previous.stencils.bed_mm : settings.stencils.bed_mm
          }
        }
      : { ...settings, tiles: previous.tiles };
  project.setSettings(JSON.stringify(active), true);
}

function applySaveSettings(
  project: Wasm.Project,
  settings: ProjectSettings,
  strict: boolean
): void {
  try {
    // Keep the exact Rust seed on every write.
    project.setSettings(JSON.stringify(settings), true);
  } catch (error) {
    // The engine left the snapshot unchanged. Downloads can use the last valid settings
    // for the inactive mode. Autosave tests that fallback only to explain what failed.
    try {
      applyActiveSettings(project, settings, 'save');
    } catch (activeError) {
      if (!strict) throw activeError;
      throw new ProjectSaveError({ kind: 'settings' }, { cause: error });
    }
    if (strict) {
      const mode = settings.mode === 'tiles' ? 'freeform' : 'tiles';
      throw new ProjectSaveError({ kind: 'inactive-settings', mode }, { cause: error });
    }
  }
}

async function applyEdits(
  project: Wasm.Project,
  state: { fontRevision: number },
  { typed, fonts }: ArtworkEdits,
  onChanged?: () => void
): Promise<boolean> {
  const colour = typed ? hexToNumber(typed.colour) : 0;
  let changed = false;
  if (
    typed &&
    (project.typedText !== typed.value ||
      project.fontMm !== typed.fontMm ||
      project.textColour !== colour)
  ) {
    project.setText(typed.value, typed.fontMm, colour);
    changed = true;
    onChanged?.();
  }
  if (fonts.revision !== state.fontRevision) {
    const files = await Promise.all(
      fonts.files.map(async ({ name, data }) => ({
        name,
        bytes: data ? new Uint8Array(await data.arrayBuffer()) : undefined
      }))
    );
    project.clearFonts();
    onChanged?.();
    for (const font of files) {
      if (font.bytes) project.addFont(font.name, font.bytes);
      else project.addBuiltInFont(font.name);
    }
    state.fontRevision = fonts.revision;
    changed = true;
  }
  return changed;
}

async function applyArtwork(workspace: Workspace, edits: ArtworkEdits): Promise<void> {
  await applyEdits(workspace.project, workspace, edits, () => {
    releaseArtwork(workspace);
  });
}

export function createHandlers(
  engine: Engine,
  drawingFonts: () => Promise<Wasm.Fonts>
): (request: Request, progress?: (stage: FreeformStage) => void) => Promise<Answer> {
  let workspace: Workspace | undefined;
  let chart: Wasm.Chart | undefined;
  let plates: Wasm.Plates | undefined;

  const opened = (): Workspace => {
    if (!workspace) throw new Error(text.errors.noArtwork);
    return workspace;
  };
  const builtChart = (): Wasm.Chart => {
    if (!chart) throw new Error(text.errors.noChart);
    return chart;
  };
  const builtPlates = (): Wasm.Plates => {
    if (!plates?.count) throw new Error(text.errors.noPlates);
    return plates;
  };

  function clearResults(): void {
    chart?.free();
    chart = undefined;
    plates?.free();
    plates = undefined;
  }

  async function artworkOf(w: Workspace): Promise<Wasm.Artwork> {
    return (w.artwork ??= w.picture
      ? engine.Artwork.fromPicture(w.picture)
      : new engine.Artwork(w.project, await drawingFonts()));
  }

  async function install(
    project: Wasm.Project,
    rawPicture?: Uint8Array,
    readArtwork: <T>(read: () => T) => T = (read) => read()
  ): Promise<Answer> {
    const next: Workspace = {
      project,
      picture: undefined,
      artwork: undefined,
      labelled: undefined,
      fontRevision: 0
    };
    const transfer: Transferable[] = [];
    try {
      let source: ArtworkSource;
      let pictureBytes: Uint8Array | undefined;
      if (project.kind === 'picture') {
        const bytes = rawPicture ?? project.sourceData();
        const picture = readArtwork(() => new engine.Source(bytes));
        next.picture = picture;
        source = {
          kind: 'picture',
          name: project.name,
          info: {
            width: picture.width,
            height: picture.height,
            pixelsX: picture.pixelsX,
            pixelsY: picture.pixelsY
          }
        };
        if (!rawPicture) {
          pictureBytes = bytes;
          transfer.push(bytes.buffer);
        }
      } else {
        const fonts = await drawingFonts();
        next.artwork = readArtwork(() => new engine.Artwork(project, fonts));
        source =
          project.kind === 'svg'
            ? { kind: 'svg', name: project.name }
            : {
                kind: 'text',
                style: {
                  value: project.typedText,
                  fontMm: project.fontMm,
                  colour: `#${project.textColour.toString(16).padStart(6, '0')}`
                }
              };
      }
      const fonts = project.fontNames().map((name, i) => {
        const bytes = project.fontBytes(i);
        return bytes ? { name, data: new Blob([bytes.slice()]) } : { name };
      });
      const value: OpenedProject = { source, settings: settingsOf(project), fonts };
      if (pictureBytes) value.pictureBytes = pictureBytes;

      clearResults();
      if (workspace) {
        releaseArtwork(workspace);
        workspace.picture?.free();
        workspace.project.free();
      }
      workspace = next;
      return { value, transfer };
    } catch (error) {
      releaseArtwork(next);
      next.picture?.free();
      project.free();
      throw error;
    }
  }

  async function freeform(
    settings: ProjectSettings,
    edits: ArtworkEdits,
    progress: ((stage: FreeformStage) => void) | undefined
  ): Promise<Answer> {
    const w = opened();
    clearResults();
    applyActiveSettings(w.project, settings, 'preview');
    await applyArtwork(w, edits);
    progress?.('paints');
    const artwork = await artworkOf(w);
    const { colours, freeform: f, stencils } = settings;
    const key = JSON.stringify([colours, f.size, f.cell_mm, f.min_patch_mm2, stencils.merges]);
    if (w.labelled?.key !== key) {
      const labels = artwork.labels(w.project);
      w.labelled?.labels.free();
      w.labelled = { key, labels };
    }
    const { labels } = w.labelled;
    progress?.('plates');
    plates = labels.plates(w.project);
    const original = artwork.preview();
    const summary: FreeformSummary = {
      widthMm: labels.widthMm,
      heightMm: labels.heightMm,
      cellMm: labels.cellMm,
      thicknessMm: settings.stencils.thickness_mm,
      background: settings.colours.background,
      palette: labels.palette(),
      counts: labels.cellCounts(),
      painted: takePreview(labels.preview()),
      plates: JSON.parse(plates.report()) as PlateReport[]
    };
    const frame = plates.frameReport(w.project);
    if (frame) summary.frame = JSON.parse(frame) as FrameReport;
    const transfer: Transferable[] = [
      summary.palette.buffer,
      summary.counts.buffer,
      summary.painted.rgba.buffer
    ];
    if (original) {
      summary.artwork = takePreview(original);
      transfer.push(summary.artwork.rgba.buffer);
    }
    return { value: summary, transfer };
  }

  return async (request, progress) => {
    switch (request.kind) {
      case 'about':
        return {
          value: { version: engine.version(), minWebMm: engine.recommendedMinWebMm() },
          transfer: []
        };
      case 'open': {
        const bytes = new Uint8Array(request.bytes);
        switch (request.file) {
          case 'picture':
            return install(engine.Project.picture(request.name, bytes), bytes);
          case 'svg':
            return install(engine.Project.svg(request.name, bytes));
          case 'project':
            return install(
              readProjectFile(() => engine.Project.open(bytes)),
              undefined,
              readProjectFile
            );
        }
        break;
      }
      case 'typeText':
        return install(engine.Project.text(request.value));
      case 'chart': {
        const { project, picture } = opened();
        clearResults();
        if (!picture) throw new Error(text.errors.tilesNeedPicture);
        applyActiveSettings(project, request.settings, 'preview');
        chart = project.tileChart(picture);
        const summary = {
          tilesX: chart.tilesX,
          tilesY: chart.tilesY,
          tilePx: chart.tilePx,
          linePx: chart.linePx,
          tileMm: chart.tileMm,
          lineMm: chart.lineMm,
          widthMm: chart.widthMm,
          heightMm: chart.heightMm,
          bedFit: engine.bedFit(chart.widthMm, chart.heightMm, ...request.settings.stencils.bed_mm),
          widthPx: chart.widthPx,
          heightPx: chart.heightPx,
          palette: chart.palette(),
          labels: chart.labels(),
          counts: chart.tileCounts(),
          stencilNames: chart.stencilNames()
        };
        return {
          value: summary,
          transfer: [summary.palette.buffer, summary.labels.buffer, summary.counts.buffer]
        };
      }
      case 'chartFile':
        return file(builtChart().chartFile(request.format, request.gridColour, request.withLines));
      case 'stencil':
        return file(builtChart().stencil(request.index, request.thicknessMm));
      case 'stencilsZip':
        return file(builtChart().stencilsZip(request.thicknessMm, Uint32Array.from(request.skip)));
      case 'freeform':
        return freeform(request.settings, request.edits, progress);
      case 'plateDrawing':
        return {
          value: JSON.parse(builtPlates().drawing(request.index)) as PlateDrawing,
          transfer: []
        };
      case 'supportIssue': {
        const issue = builtPlates().supportIssue(
          request.index,
          Int32Array.from(request.ends.flat()),
          request.edit
        );
        return { value: issue as SupportIssue | undefined, transfer: [] };
      }
      case 'paintPaths':
        return { value: JSON.parse(plates?.paintPaths() ?? '[]') as Coat[], transfer: [] };
      case 'plateFile':
        return file(builtPlates().stl(request.index, opened().project));
      case 'frameFile':
        return file(builtPlates().frameStl(opened().project));
      case 'platesZip':
        return file(builtPlates().zip(opened().project));
      case 'threeMf':
        return file(builtPlates().threeMf(opened().project));
      case 'projectFile': {
        const w = opened();
        // Save a snapshot copy, so a queued autosave never disturbs the live preview.
        const snapshot = w.project.clone();
        try {
          applySaveSettings(snapshot, request.settings, request.strict === true);
          const changed = await applyEdits(
            snapshot,
            { fontRevision: w.fontRevision },
            request.edits
          );
          // Validate changed text or fonts on the copy; an unchanged drawing was already
          // validated when it opened, and its live artwork is still current.
          if (snapshot.kind !== 'picture' && (changed || !w.artwork)) {
            new engine.Artwork(snapshot, await drawingFonts()).free();
          }
          return file(snapshot.file());
        } catch (error) {
          if (!request.strict || error instanceof ProjectSaveError) throw error;
          throw new ProjectSaveError({ kind: 'artwork' }, { cause: error });
        } finally {
          snapshot.free();
        }
      }
      case 'validateProject': {
        let project: Wasm.Project;
        try {
          project = engine.Project.open(new Uint8Array(request.bytes));
        } catch {
          return { value: null, transfer: [] };
        }
        try {
          // A failed font load says nothing about the file. Let it reject the request.
          const fonts = project.kind === 'picture' ? undefined : await drawingFonts();
          try {
            if (fonts) new engine.Artwork(project, fonts).free();
            else new engine.Source(project.sourceData()).free();
          } catch {
            return { value: null, transfer: [] };
          }
          return { value: settingsOf(project).mode, transfer: [] };
        } finally {
          project.free();
        }
      }
    }
    throw new Error(text.errors.unknownRequest);
  };
}
