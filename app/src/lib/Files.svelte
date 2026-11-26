<script lang="ts">
  // Everything there is to save: the chart in three formats, with or without grid lines,
  // and the stencils, together or one at a time.
  import type { ChartFormat } from '../engine/client';
  import { chartFileName, stencilFileName, stencilsZipName } from './names';
  import Segmented from './Segmented.svelte';
  import TileCanvas from './TileCanvas.svelte';
  import { text } from './text';
  import type { Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
    dark: boolean;
  }

  let { bench, dark }: Props = $props();

  /** Larger than this on either side, and a stencil fits few printers. */
  const COMMON_BED_MM = 256;
  const CARD_WIDTH = 150;

  let withLines = $state(true);
  let previewWidth = $state(0);
  let cardWidth = $state(CARD_WIDTH);

  const chart = $derived(bench.chart);
  const s = $derived(bench.settings);
  const STYLES = [
    { value: 'lines', label: text.files.withLines },
    { value: 'tiles', label: text.files.tilesOnly }
  ] as const;

  /** Pixel size of the chart file, which loses its lines when they are left out. */
  const pixels = $derived.by(() => {
    if (!chart) return { width: 0, height: 0 };
    if (withLines) return { width: chart.widthPx, height: chart.heightPx };
    return { width: chart.tilesX * chart.tilePx, height: chart.tilesY * chart.tilePx };
  });

  const formats = $derived<{ format: ChartFormat; title: string; body: string }[]>(
    chart
      ? [
          {
            format: 'pdf',
            title: text.files.pdf,
            body: text.files.pdfBody(chart.widthMm, chart.heightMm)
          },
          {
            format: 'png',
            title: text.files.png,
            body: text.files.pngBody(pixels.width, pixels.height, Math.round(s.dpi))
          },
          { format: 'jpeg', title: text.files.jpeg, body: text.files.jpegBody }
        ]
      : []
  );
</script>

{#if chart}
  <div class="files">
    <header class="top">
      <h1 class="title">{text.files.heading}</h1>
      <p class="note">
        {text.files.summary(bench.fileName, chart.tilesX, chart.tilesY, chart.counts.length)}
      </p>
    </header>

    <div class="columns">
      <section class="column chart" aria-labelledby="chart-files-heading">
        <div class="row">
          <h2 id="chart-files-heading" class="group">{text.files.chart}</h2>
          <Segmented
            label={text.files.style}
            options={STYLES}
            value={withLines ? 'lines' : 'tiles'}
            onpick={(style) => {
              withLines = style === 'lines';
            }}
          />
        </div>
        <div class="measure" bind:clientWidth={previewWidth}>
          <TileCanvas
            {chart}
            colours={bench.colours}
            width={Math.min(previewWidth, 520)}
            gridColour={s.gridColour}
            {withLines}
            label={text.bench.chartLabel(chart.tilesX, chart.tilesY)}
          />
        </div>
        <ul class="list">
          {#each formats as item (item.format)}
            {@const name = chartFileName(bench.base, item.format, withLines)}
            <li>
              <div class="about">
                <p class="what">
                  <strong>{item.title}</strong>
                  {#if item.format === 'pdf'}<span class="badge">{text.files.best}</span>{/if}
                </p>
                <p class="body">{item.body}</p>
                <p class="mono name">{name}</p>
              </div>
              <button
                type="button"
                class="button"
                class:primary={item.format === 'pdf'}
                disabled={!bench.ready}
                onclick={() => void bench.saveChart(item.format, withLines)}
              >
                {bench.saving === name ? text.files.saving : text.files.save(item.title)}
              </button>
            </li>
          {/each}
        </ul>
      </section>

      <section class="column" aria-labelledby="stencil-files-heading">
        <h2 id="stencil-files-heading" class="group">{text.files.stencils}</h2>
        {#if bench.stencilsPossible}
          {@const zip = stencilsZipName(bench.base)}
          <div class="all">
            <div class="about">
              <p class="what"><strong>{text.files.all(chart.counts.length)}</strong></p>
              <p class="body">{text.files.allBody(s.thicknessMm)}</p>
              <p class="mono name">{zip}</p>
            </div>
            <button
              type="button"
              class="button primary"
              disabled={!bench.ready}
              onclick={() => void bench.saveStencils()}
            >
              {bench.saving === zip ? text.files.saving : text.files.saveAll}
            </button>
          </div>

          <ul class="cards">
            {#each bench.colours as colour, i (i)}
              {@const name = stencilFileName(bench.base, chart.stencilNames[i] ?? '')}
              <li>
                <div class="measure" bind:clientWidth={cardWidth}>
                  <TileCanvas
                    {chart}
                    colours={bench.colours}
                    width={cardWidth}
                    stencil={i}
                    {dark}
                    label={text.stencil.label(i, colour, chart.counts[i] ?? 0)}
                  />
                </div>
                <p class="card-line">
                  <span class="swatch" style:background={colour}></span>
                  <span class="mono hex">{colour.toUpperCase()}</span>
                  <span class="holes">{text.stencil.holes(chart.counts[i] ?? 0)}</span>
                </p>
                <button
                  type="button"
                  class="button small"
                  aria-label={text.files.saveOne(i, colour)}
                  disabled={!bench.ready}
                  onclick={() => void bench.saveStencil(i)}
                >
                  {bench.saving === name ? text.files.saving : text.files.saveStl}
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="notice">{text.files.none}</p>
        {/if}

        <div class="advice">
          <h3 class="label">{text.files.beforeYouPrint}</h3>
          <ol>
            <li>{text.files.actualSize}</li>
            {#if bench.stencilsPossible}<li>{text.files.flat(chart.lineMm)}</li>{/if}
          </ol>
          {#if bench.stencilsPossible && Math.max(chart.widthMm, chart.heightMm) > COMMON_BED_MM}
            <p class="notice">{text.files.tooLarge(chart.widthMm, chart.heightMm)}</p>
          {/if}
        </div>
      </section>
    </div>
  </div>
{/if}

<style>
  .files {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 28px 32px 40px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .top {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .title {
    font-family: var(--serif);
    font-size: 34px;
    font-weight: 400;
    letter-spacing: -0.02em;
    line-height: 1.1;
  }

  .columns {
    display: flex;
    flex-wrap: wrap;
    gap: 40px;
    align-items: flex-start;
  }

  .column {
    flex: 1 1 340px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .chart {
    flex-grow: 0.8;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
  }

  .group {
    font-size: 22px;
  }

  .list {
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--line);
  }

  .list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    padding: 14px 0;
    border-bottom: 1px solid var(--line);
  }

  .about {
    flex: 1 1 240px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .what {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .what strong {
    font-weight: 600;
  }

  .badge {
    padding: 1px 6px;
    border: 1px solid var(--tint-line);
    background: var(--tint);
    color: var(--tint-text);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .body {
    color: var(--text-2);
  }

  .name {
    font-size: 12px;
    color: var(--text-3);
    overflow-wrap: anywhere;
  }

  .all {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 20px;
    padding: 16px;
    border: 1px solid var(--line);
    background: var(--surface);
  }

  .cards {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 20px 16px;
  }

  .cards li {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .card-line {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  .swatch {
    width: 14px;
    height: 14px;
    flex: none;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.2);
  }

  .hex {
    flex: 1;
  }

  .holes {
    color: var(--text-2);
  }

  .small {
    min-height: 34px;
    font-size: 11px;
  }

  .advice {
    padding-top: 16px;
    border-top: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .advice ol {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .advice li::marker {
    color: var(--tint-text);
    font-weight: 700;
  }
</style>
