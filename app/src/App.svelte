<script lang="ts">
  // The working shell: every feature wired to the engine, with no design yet.
  import { onDestroy } from 'svelte';

  import {
    Engine,
    type ChartFormat,
    type ChartSettings,
    type ChartSummary,
    type Mode,
    type SourceInfo
  } from './engine/client';
  import ChartCanvas from './lib/ChartCanvas.svelte';
  import { hexToNumber, rgbToHex } from './lib/colour';
  import { save } from './lib/download';
  import { latest } from './lib/latest';

  const MIN_WEB_MM = 0.6;
  const TYPES: Record<ChartFormat, string> = {
    png: 'image/png',
    jpeg: 'image/jpeg',
    pdf: 'application/pdf'
  };

  const engine = new Engine();
  const build = latest((settings: ChartSettings) => engine.chart(settings));

  let version = $state('');
  let source: SourceInfo | undefined = $state();
  let fileName = $state('');
  let original = $state('');
  let chart: ChartSummary | undefined = $state();
  let current = $state(false);
  let problem = $state('');
  let busy = $state(false);

  let canvasWidthMm = $state(200);
  let canvasHeightMm = $state(200);
  let tileMm = $state(5);
  let lineMm = $state(0.8);
  let dpi = $state(300);
  let mode: Mode = $state('colour');
  let colours = $state(16);
  let background = $state('#ffffff');
  let gridColour = $state('#000000');
  let thicknessMm = $state(1.2);

  void engine.version().then((v) => (version = v));
  onDestroy(() => {
    if (original) URL.revokeObjectURL(original);
  });

  const baseName = $derived(fileName.replace(/\.[^.]*$/, '') || 'stencilator');

  async function open(file: File | undefined) {
    if (!file) return;
    problem = '';
    try {
      source = await engine.open(file);
      fileName = file.name;
      if (original) URL.revokeObjectURL(original);
      original = URL.createObjectURL(file);
    } catch (error) {
      problem = error instanceof Error ? error.message : String(error);
    }
  }

  $effect(() => {
    if (!source) return;
    const settings: ChartSettings = {
      canvasWidthMm,
      canvasHeightMm,
      tileMm,
      lineMm,
      dpi,
      mode,
      colours,
      background: hexToNumber(background)
    };
    current = false;
    build(settings).then(
      (outcome) => {
        if (outcome.superseded) return;
        chart = outcome.result;
        problem = '';
        current = true;
      },
      (error: unknown) => {
        chart = undefined;
        problem = error instanceof Error ? error.message : String(error);
        current = true;
      }
    );
  });

  async function download(task: () => Promise<Uint8Array>, name: string, type: string) {
    busy = true;
    try {
      save(await task(), name, type);
    } catch (error) {
      problem = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  const saveChart = (format: ChartFormat, withLines: boolean) =>
    download(
      () => engine.chartFile(format, hexToNumber(gridColour), withLines),
      `${baseName}-${withLines ? 'chart' : 'tiles'}.${format === 'jpeg' ? 'jpg' : format}`,
      TYPES[format]
    );

  const ready = $derived(chart !== undefined && current && !busy);
  const stencilsPossible = $derived(chart !== undefined && chart.linePx > 0);
</script>

<main>
  <h1>Stencilator</h1>

  <p>
    <label>
      Image
      <input
        type="file"
        accept="image/png,image/jpeg,image/webp"
        onchange={(event) => open(event.currentTarget.files?.[0])}
      />
    </label>
    {#if source}{fileName}, {source.width} × {source.height} px{/if}
  </p>

  <fieldset>
    <legend>Canvas</legend>
    <label>Width (mm) <input type="number" bind:value={canvasWidthMm} step="1" /></label>
    <label>Height (mm) <input type="number" bind:value={canvasHeightMm} step="1" /></label>
    <label>Tile (mm) <input type="number" bind:value={tileMm} step="0.1" /></label>
    <label>DPI <input type="number" bind:value={dpi} step="50" /></label>
  </fieldset>

  <fieldset>
    <legend>Colours</legend>
    <label>
      Mode
      <select bind:value={mode}>
        <option value="colour">Colour</option>
        <option value="greyscale">Greyscale</option>
        <option value="black-white">Black and white</option>
      </select>
    </label>
    <label>
      Colours <input type="number" bind:value={colours} disabled={mode === 'black-white'} />
    </label>
    <label>Background <input type="color" bind:value={background} /></label>
  </fieldset>

  <fieldset>
    <legend>Grid and stencils</legend>
    <label>Line width (mm) <input type="number" bind:value={lineMm} step="0.1" /></label>
    <label>Line colour <input type="color" bind:value={gridColour} /></label>
    <label>
      Stencil thickness (mm) <input type="number" bind:value={thicknessMm} step="0.1" />
    </label>
  </fieldset>

  {#if problem}<p role="alert">{problem}</p>{/if}

  {#if source}
    <div class="previews">
      <figure>
        <img src={original} alt="The original" />
        <figcaption>Original</figcaption>
      </figure>
      {#if chart}
        <figure class:stale={!current}>
          <ChartCanvas {chart} {gridColour} withLines={true} />
          <figcaption>Chart</figcaption>
        </figure>
      {/if}
    </div>
  {/if}

  {#if chart}
    <p class="summary">
      {chart.tilesX} × {chart.tilesY} tiles ({chart.tilesX * chart.tilesY}), {chart.counts.length}
      colours. {chart.widthMm.toFixed(1)} × {chart.heightMm.toFixed(1)} mm, tile {chart.tileMm.toFixed(
        2
      )} mm, line {chart.lineMm.toFixed(2)} mm.
      {#if chart.linePx === 0}
        No grid lines, so no stencils: their holes would merge.
      {:else if chart.lineMm < MIN_WEB_MM}
        Grid lines under {MIN_WEB_MM} mm make fragile stencils.
      {/if}
    </p>

    <ol class="palette">
      {#each chart.stencilNames as name, i (name)}
        <li>
          <span
            class="swatch"
            style:background={rgbToHex(
              chart.palette[3 * i] ?? 0,
              chart.palette[3 * i + 1] ?? 0,
              chart.palette[3 * i + 2] ?? 0
            )}
          ></span>
          {chart.counts[i]} tiles
          <button
            disabled={!ready || !stencilsPossible}
            onclick={() => download(() => engine.stencil(i, thicknessMm), name, 'model/stl')}
          >
            STL
          </button>
        </li>
      {/each}
    </ol>

    <p class="actions">
      {#each ['png', 'pdf', 'jpeg'] as const as format (format)}
        <button disabled={!ready} onclick={() => saveChart(format, true)}>
          Save chart ({format.toUpperCase()})
        </button>
      {/each}
      <button disabled={!ready} onclick={() => saveChart('pdf', false)}>
        Save tiles only (PDF)
      </button>
      <button
        disabled={!ready || !stencilsPossible}
        onclick={() =>
          download(
            () => engine.stencilsZip(thicknessMm),
            `${baseName}-stencils.zip`,
            'application/zip'
          )}
      >
        Save all stencils (zip)
      </button>
    </p>
  {/if}

  <footer>Engine {version}</footer>
</main>

<style>
  main {
    max-width: 72rem;
    margin: 0 auto;
    padding: 1rem;
  }

  fieldset {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    margin-bottom: 0.75rem;
  }

  .previews {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  figure {
    margin: 0;
  }

  img {
    width: 100%;
    image-rendering: pixelated;
  }

  .stale {
    opacity: 0.5;
  }

  .palette {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
  }

  .swatch {
    display: inline-block;
    width: 1em;
    height: 1em;
    vertical-align: middle;
    border: 1px solid currentColor;
  }
</style>
