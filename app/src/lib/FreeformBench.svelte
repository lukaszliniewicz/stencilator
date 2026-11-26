<script lang="ts">
  import { CAPTION_SPACE, RULER_SPACE } from './bench-space';
  import Raster from './Raster.svelte';
  import Sheet from './Sheet.svelte';
  import { text } from './text';
  import type { Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
    choose: () => void;
  }

  let { bench, choose }: Props = $props();
  const words = text.freeform;
  const PAIR_GAP = 24;
  let room: DOMRectReadOnly | undefined = $state();
  let islands = $state(true);
  let webs = $state(true);
  let openings = $state(true);

  const width = $derived(room?.width ?? 0);
  const summary = $derived(bench.freeform);
  const plate = $derived(bench.selectedPlate);
  const drawing = $derived(bench.drawing);
  const faded = $derived(!bench.current || !bench.valid || bench.opening);
  const originalAspect = $derived.by(() => {
    if (bench.source) return bench.source.width / bench.source.height;
    const original = summary?.artwork;
    return original ? original.width / original.height : 1;
  });
  const stage = $derived(bench.progress === 'plates' ? words.building : words.finding);

  function fittedWidth(aspect: number, availableWidth: number, rulers = false): number {
    const border = rulers ? RULER_SPACE : 0;
    const height = (room?.height ?? 0) - CAPTION_SPACE - border;
    return Math.max(0, Math.floor(Math.min(availableWidth - border, height * aspect)));
  }
</script>

{#snippet artwork(available: number)}
  {@const width = fittedWidth(originalAspect, available)}
  <figure class="figure" class:faded={faded && !bench.pictureUrl}>
    <figcaption class="label">{words.artwork}</figcaption>
    <div
      class="picture"
      style:width="{width}px"
      style:height="{width / originalAspect}px"
      style:background={bench.settings.background}
    >
      {#if bench.pictureUrl}
        <img src={bench.pictureUrl} alt={text.picture.alt} />
      {:else if summary?.artwork}
        <Raster preview={summary.artwork} label={words.artworkAlt} />
      {/if}
    </div>
  </figure>
{/snippet}

{#snippet paints(available: number)}
  {#if summary}
    {@const aspect = summary.widthMm / summary.heightMm}
    {@const width = fittedWidth(aspect, available)}
    <figure class="figure" class:faded>
      <figcaption class="label">
        {words.views.chart} · {words.artworkSize(summary.widthMm, summary.heightMm)}
      </figcaption>
      <div class="picture" style:width="{width}px" style:height="{width / aspect}px">
        <Raster preview={summary.painted} label={words.painted} />
      </div>
    </figure>
  {/if}
{/snippet}

<div class="bench">
  <div class="area" bind:contentRect={room}>
    {#if !bench.artwork}
      <div class="empty">
        <span class="label">{text.project.modes.freeform}</span>
        <h1>{words.drop}</h1>
        <p>{words.dropBody}</p>
        <div class="actions">
          <button type="button" class="button primary" disabled={!bench.canOpen} onclick={choose}>
            {words.choose}
          </button>
          <button type="button" class="button" disabled={!bench.canOpen} onclick={bench.typeText}>
            {words.type}
          </button>
          <button
            type="button"
            class="button"
            disabled={!bench.canOpen}
            onclick={bench.openExample}
          >
            {words.example}
          </button>
        </div>
      </div>
    {:else if bench.view === 'stencil' && summary}
      {#if drawing && plate}
        <Sheet
          widthMm={drawing.width_mm}
          heightMm={drawing.height_mm}
          width={fittedWidth(drawing.width_mm / drawing.height_mm, width, true)}
          start={words.plateFor(plate.paint, plate.colour)}
          end={words.openings(plate.openings)}
          {faded}
        >
          <svg
            class="plate"
            viewBox="0 0 {drawing.width_mm} {drawing.height_mm}"
            role="img"
            aria-label={words.plateFor(plate.paint, plate.colour)}
          >
            <path class="material" d={drawing.plate} fill-rule="evenodd" />
            {#if islands}<path class="islands" d={drawing.islands} fill-rule="evenodd" />{/if}
            {#if webs}<path class="webs" d={drawing.thin_webs} fill-rule="evenodd" />{/if}
            {#if openings}<path
                class="openings"
                d={drawing.narrow_openings}
                fill-rule="evenodd"
              />{/if}
          </svg>
        </Sheet>
      {:else}
        <p class="note">
          {bench.progress ? stage : plate ? words.previewing : words.nothingSelected}
        </p>
      {/if}
    {:else if bench.view === 'both' && summary}
      {@const half = (width - PAIR_GAP) / 2}
      <div class="pair">
        {@render artwork(half)}
        {@render paints(half)}
      </div>
    {:else if bench.view === 'picture' || !summary}
      {#if bench.pictureUrl || summary?.artwork}
        {@render artwork(width)}
      {:else if bench.progress}
        <p class="note">{stage}</p>
      {/if}
    {:else}
      {@render paints(width)}
    {/if}
  </div>

  {#if bench.view === 'stencil' && drawing}
    <div class="overlays" role="group" aria-label={words.highlights}>
      <label>
        <input type="checkbox" bind:checked={islands} class="island-choice" />{words.islandLayer}
      </label>
      <label>
        <input type="checkbox" bind:checked={webs} class="web-choice" />{words.webLayer}
      </label>
      <label>
        <input type="checkbox" bind:checked={openings} class="opening-choice" />{words.openingLayer}
      </label>
    </div>
  {/if}
  <p class="status" role="status">
    {#if bench.opening}
      {text.project.opening}
    {:else if bench.progress}
      {stage}
    {:else if summary}
      {words.summary(
        summary.widthMm,
        summary.heightMm,
        summary.counts.length,
        summary.plates.length,
        summary.cellMm
      )}
    {:else}
      {text.project.freeformNote}
    {/if}
  </p>
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

  .area {
    flex: 1;
    min-height: 0;
    padding: 28px;
    display: grid;
    place-items: center;
    overflow: hidden;
  }

  .empty {
    max-width: 470px;
    padding: 28px;
    border: 1px dashed var(--line-strong);
    background: var(--panel);
    text-align: center;
  }

  .empty h1 {
    margin: 5px 0 8px;
    font: 400 34px/1.1 var(--serif);
    letter-spacing: -0.02em;
  }

  .empty p {
    color: var(--text-2);
  }

  .actions,
  .pair {
    display: flex;
    justify-content: center;
  }

  .actions {
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 18px;
  }

  .pair {
    gap: 24px;
    align-items: flex-start;
  }

  .figure {
    margin: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition: opacity 0.2s 0.15s;
  }

  .figure .label {
    line-height: 18px;
  }

  .picture {
    box-shadow: 0 0 0 1px var(--line);
  }

  .picture img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .faded {
    opacity: 0.4;
  }

  .plate {
    display: block;
    width: 100%;
    height: 100%;
  }

  .material {
    fill: light-dark(#404040, #7a7a7a);
  }

  .islands {
    fill: light-dark(#d97706, #f4aa44);
  }

  .webs {
    fill: light-dark(#dc2626, #ff6262);
  }

  .openings {
    fill: light-dark(#2563eb, #60a5fa);
  }

  .overlays {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 6px 16px;
    padding: 0 12px 14px;
    font-size: 12px;
    color: var(--text-2);
  }

  .overlays label {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .island-choice {
    accent-color: #d97706;
  }

  .web-choice {
    accent-color: #dc2626;
  }

  .opening-choice {
    accent-color: var(--tint-text);
  }

  .status {
    padding: 8px 16px;
    border-top: 1px solid var(--line);
    background: var(--panel);
    color: var(--text-2);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  @media (max-width: 720px) {
    .area {
      padding: 16px;
    }
  }
</style>
