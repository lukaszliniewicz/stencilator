<script lang="ts">
  // The picture, chart or stencil, fitted to the room there is.
  import { CAPTION_SPACE, RULER_SPACE } from './bench-space';
  import Sheet from './Sheet.svelte';
  import TileCanvas from './TileCanvas.svelte';
  import { text } from './text';
  import Viewport from './Viewport.svelte';
  import type { Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
    dark: boolean;
  }

  let { bench, dark }: Props = $props();

  const SIDE_BY_SIDE_GAP = 32;

  // The room inside the bench's padding, which is narrower on small screens.
  let room: DOMRectReadOnly | undefined = $state();
  const roomWidth = $derived(room?.width ?? 0);
  const roomHeight = $derived(room?.height ?? 0);

  /** The widest a sheet of this shape can be in the room, rulers and caption included. */
  function sheetWidth(widthMm: number, heightMm: number): number {
    if (!(widthMm > 0 && heightMm > 0)) return 0;
    const width = roomWidth - RULER_SPACE;
    const height = roomHeight - RULER_SPACE - CAPTION_SPACE;
    return Math.max(0, Math.floor(Math.min(width, (height * widthMm) / heightMm)));
  }

  /** The widest a plain figure can be in a box, its caption included. */
  function figureWidth(aspect: number, width: number, height: number): number {
    return Math.max(0, Math.floor(Math.min(width, (height - CAPTION_SPACE) * aspect)));
  }

  const s = $derived(bench.settings);
  const chart = $derived(bench.chart);
  const faded = $derived(!bench.current || !bench.valid);
  const view = $derived(bench.view === 'stencil' && !bench.stencilsPossible ? 'chart' : bench.view);
  const pictureAspect = $derived(bench.source ? bench.source.width / bench.source.height : 1);
</script>

{#snippet picture(width: number)}
  <figure class="figure">
    <figcaption class="label">
      <span>{text.picture.heading} · {bench.fileName}</span>
      {#if bench.source}
        <span>{text.picture.pixels(bench.source.width, bench.source.height)}</span>
      {/if}
    </figcaption>
    <img
      src={bench.pictureUrl}
      alt={text.picture.alt}
      class:pixelated={width > (bench.source?.width ?? 0)}
      style:width="{width}px"
      style:height="{width / pictureAspect}px"
    />
  </figure>
{/snippet}

<div class="bench">
  <Viewport bind:room frameKey={[bench.artworkVersion, view].join('-')}>
    {#if view === 'picture' || !chart}
      {@render picture(figureWidth(pictureAspect, roomWidth, roomHeight))}
    {:else if view === 'both'}
      {@const half = (roomWidth - SIDE_BY_SIDE_GAP) / 2}
      {@const aspect = chart.widthMm / chart.heightMm}
      <div class="pair">
        {@render picture(figureWidth(pictureAspect, half, roomHeight))}
        <figure class="figure" class:faded>
          <figcaption class="label">
            <span>{text.bench.chart(chart.widthMm, chart.heightMm)}</span>
          </figcaption>
          <TileCanvas
            {chart}
            colours={bench.colours}
            width={figureWidth(aspect, half, roomHeight)}
            gridColour={s.gridColour}
            label={text.bench.chartLabel(chart.tilesX, chart.tilesY)}
          />
        </figure>
      </div>
    {:else if view === 'stencil'}
      {@const index = bench.selected}
      <Sheet
        widthMm={chart.widthMm}
        heightMm={chart.heightMm}
        width={sheetWidth(chart.widthMm, chart.heightMm)}
        start={text.bench.stencilCaption(index, bench.colours[index] ?? '')}
        end={bench.unstencilled.includes(bench.colours[index] ?? '')
          ? text.stencil.none
          : text.stencil.holes(chart.counts[index] ?? 0)}
        {faded}
      >
        {#snippet children({ width }: { width: number })}
          <TileCanvas
            {chart}
            colours={bench.colours}
            {width}
            stencil={index}
            {dark}
            label={text.stencil.label(index, bench.colours[index] ?? '', chart.counts[index] ?? 0)}
          />
        {/snippet}
      </Sheet>
    {:else}
      <Sheet
        widthMm={chart.widthMm}
        heightMm={chart.heightMm}
        width={sheetWidth(chart.widthMm, chart.heightMm)}
        start={text.bench.chart(chart.widthMm, chart.heightMm)}
        end={s.sizing === 'pixels'
          ? text.bench.fromPixels(chart.tilesX, chart.tilesY)
          : text.bench.onCanvas(s.canvasWidthMm, s.canvasHeightMm)}
        {faded}
      >
        {#snippet children({ width }: { width: number })}
          <TileCanvas
            {chart}
            colours={bench.colours}
            {width}
            gridColour={s.gridColour}
            label={text.bench.chartLabel(chart.tilesX, chart.tilesY)}
          />
        {/snippet}
      </Sheet>
    {/if}
  </Viewport>

  {#if chart}
    <p class="status">
      {text.bench.status(
        chart.tilesX,
        chart.tilesY,
        chart.counts.length,
        chart.tileMm,
        chart.lineMm
      )}
    </p>
  {:else if bench.source}
    <p class="status">{text.picture.pixels(bench.source.width, bench.source.height)}</p>
  {/if}
</div>

<style>
  .bench {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background-color: var(--surface);
    background-image:
      linear-gradient(var(--grid) 1px, transparent 1px),
      linear-gradient(90deg, var(--grid) 1px, transparent 1px);
    background-size: 20px 20px;
  }

  .pair {
    display: flex;
    align-items: center;
    gap: 32px;
  }

  .figure {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition: opacity 0.2s 0.15s;
  }

  .figure figcaption {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    line-height: 18px;
  }

  .figure img {
    display: block;
    box-shadow: 0 0 0 1px var(--line);
  }

  .pixelated {
    image-rendering: pixelated;
  }

  .faded {
    opacity: 0.4;
  }

  .status {
    padding: 8px 16px;
    border-top: 1px solid var(--line);
    background: var(--panel);
    color: var(--text-2);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
</style>
