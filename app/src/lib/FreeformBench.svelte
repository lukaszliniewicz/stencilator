<script lang="ts">
  import { CAPTION_SPACE, RULER_SPACE } from './bench-space';
  import PaintedResult from './PaintedResult.svelte';
  import PlateTop from './PlateTop.svelte';
  import Raster from './Raster.svelte';
  import Segmented from './Segmented.svelte';
  import Sheet from './Sheet.svelte';
  import { text } from './text';
  import Viewport from './Viewport.svelte';
  import type { Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
    download: () => Promise<void>;
    dark: boolean;
  }

  let { bench, download, dark }: Props = $props();
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
  const plateViews = (['top', '3d'] as const).map((value) => ({
    value,
    label: text.printPreview.views[value]
  }));

  function fittedWidth(aspect: number, availableWidth: number, rulers = false, extra = 0): number {
    const border = rulers ? RULER_SPACE : 0;
    const height = (room?.height ?? 0) - CAPTION_SPACE - border - extra;
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

{#snippet paintedResult(available: number)}
  {#if summary}
    <PaintedResult
      {summary}
      coats={bench.coatPaths}
      width={fittedWidth(summary.widthMm / summary.heightMm, available, false, 98)}
      {faded}
    />
  {/if}
{/snippet}

{#snippet selectedPlate(available: number)}
  {#if plate && bench.plateView === '3d'}
    {#key plate.paint}
      {#await import('./PrintPreview.svelte')}
        <p class="note">{text.printPreview.loading}</p>
      {:then { default: Preview }}
        <Preview
          {bench}
          {plate}
          {dark}
          width={available}
          height={Math.max(120, Math.floor((room?.height ?? 0) - 124))}
        />
      {:catch}
        <div class="notice">
          <p>{text.workspace.oldViewHelp}</p>
          <div class="button-row">
            <button
              type="button"
              class="quiet"
              disabled={!bench.canSaveProject}
              onclick={() => void download()}
            >
              {text.project.save}
            </button>
            <button
              type="button"
              class="button"
              onclick={() => {
                window.location.reload();
              }}
            >
              {text.workspace.reload}
            </button>
          </div>
        </div>
      {/await}
    {/key}
  {:else if drawing && plate}
    <Sheet
      widthMm={drawing.width_mm}
      heightMm={drawing.height_mm}
      width={fittedWidth(drawing.width_mm / drawing.height_mm, available, true)}
      start={words.plateFor(plate.paint, plate.colour)}
      end={words.openings(plate.openings)}
      {faded}
      bottomOrigin
    >
      {#snippet children({ width })}
        <PlateTop {bench} {drawing} {plate} {islands} {webs} {openings} {width} />
      {/snippet}
    </Sheet>
  {:else}
    <p class="note">
      {bench.progress ? stage : plate ? words.previewing : words.nothingSelected}
    </p>
  {/if}
{/snippet}

<div class="bench">
  {#if bench.view === 'stencil' && summary}
    <div class="view-options">
      <Segmented
        label={text.printPreview.view}
        options={plateViews}
        value={bench.plateView}
        onpick={bench.showPlate}
      />
      <label class="alongside note">
        <input type="checkbox" bind:checked={bench.comparePaint} />{text.coating.alongside}
      </label>
    </div>
  {/if}
  <Viewport
    bind:room
    enabled={bench.view !== 'stencil' || bench.plateView !== '3d'}
    frameKey={[
      bench.artworkVersion,
      bench.view,
      bench.plateView,
      bench.selected,
      bench.comparePaint
    ].join('-')}
  >
    {#if bench.view === 'stencil' && summary}
      {#if bench.comparePaint}
        {@const half = Math.max(0, (width - PAIR_GAP) / 2)}
        <div class="pair">
          {@render selectedPlate(half)}
          {@render paintedResult(half)}
        </div>
      {:else}
        {@render selectedPlate(width)}
      {/if}
    {:else if bench.view === 'result' && summary}
      {@render paintedResult(width)}
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
  </Viewport>

  {#if bench.view === 'stencil' && bench.plateView === 'top' && drawing}
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
  {#if bench.supportDraft}
    <div class="support-actions">
      {#if bench.supportProblem}<p class="notice" role="alert">{bench.supportProblem}</p>{/if}
      <div class="button-row">
        <button
          type="button"
          class="button primary"
          disabled={bench.checkingSupport || !bench.ready}
          onclick={bench.applySupport}
        >
          {bench.checkingSupport ? text.supports.checking : text.supports.apply}
        </button>
        <button type="button" class="button" onclick={bench.cancelSupport}>
          {text.supports.cancel}
        </button>
      </div>
    </div>
  {/if}
  <p class="status" role="status">
    {#if bench.opening}
      {text.project.opening}
    {:else if bench.progress}
      {stage}
    {:else if bench.supportDraft}
      {bench.supportEndsSet ? text.supports.placed : text.supports.placing(bench.supportDraft.next)}
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

  .view-options {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 14px;
    align-items: center;
    padding: 10px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }

  .support-actions {
    display: none;
  }

  @media (max-width: 1100px) {
    .support-actions {
      display: grid;
      gap: 6px;
      padding: 6px 10px;
      border-top: 1px solid var(--line);
      background: var(--panel);
    }
  }

  .alongside {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .pair {
    display: flex;
    justify-content: center;
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
</style>
