<script lang="ts">
  // Artwork, shared paint controls, and the selected stencil or plate report.
  import ArtworkControls from './ArtworkControls.svelte';
  import PlateIssues from './PlateIssues.svelte';
  import Segmented from './Segmented.svelte';
  import TileCanvas from './TileCanvas.svelte';
  import { num, text } from './text';
  import type { Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
    dark: boolean;
    choose: () => void;
    typeText: () => void;
  }

  let { bench, dark, choose, typeText }: Props = $props();

  let previewWidth = $state(0);
  const chart = $derived(bench.chart);
  const freeform = $derived(bench.isFreeform ? bench.freeform : undefined);
  const paints = $derived(bench.isFreeform ? freeform : chart);
  const picked = $derived(bench.selected);
  const pickedColour = $derived(bench.colours[picked] ?? '');
  const merges = $derived(bench.settings.merges);
  const plateByPaint = $derived(
    new Map(freeform?.plates.map((plate, i) => [plate.paint, { plate, i }]))
  );
  const rows = $derived([
    ...plateByPaint.keys(),
    ...bench.colours.map((_, i) => i).filter((i) => !plateByPaint.has(i))
  ]);
  const coatPosition = $derived(plateByPaint.get(picked)?.i ?? -1);
  const palettes = (['source', 'ral'] as const).map((value) => ({
    value,
    label: text.finishing.palettes[value]
  }));
</script>

<div class="sidebar">
  {#if bench.isFreeform}
    <section aria-labelledby="artwork-heading">
      <h2 id="artwork-heading" class="label heading">{text.freeform.artwork}</h2>
      <ArtworkControls {bench} {choose} {typeText} />
    </section>
  {:else}
    <section aria-labelledby="picture-heading">
      <h2 id="picture-heading" class="label heading">{text.picture.heading}</h2>
      {#if bench.source}
        <img class="thumbnail" src={bench.pictureUrl} alt={text.picture.alt} />
        <p class="file">
          <span class="name">{bench.fileName}</span>
          <span class="note">{text.picture.pixels(bench.source.width, bench.source.height)}</span>
        </p>
        <button type="button" class="button" disabled={!bench.canOpen} onclick={choose}>
          {text.picture.replace}
        </button>
      {/if}
    </section>
  {/if}

  {#if paints}
    <section aria-labelledby="paints-heading">
      <h2 id="paints-heading" class="label heading split">
        <span>{text.paints.heading}</span>
        <span class="aside">
          {freeform ? text.coating.heading : text.paints.order(paints.counts.length)}
        </span>
      </h2>
      <ul class="paints">
        {#each rows as i (i)}
          {@const colour = bench.colours[i] ?? ''}
          {@const count = paints.counts[i] ?? 0}
          {@const area = freeform ? count * freeform.cellMm ** 2 : 0}
          {@const coat = plateByPaint.get(i)}
          <li class:off={bench.unstencilled.includes(colour)}>
            <button
              type="button"
              aria-pressed={picked === i}
              aria-label={freeform
                ? text.freeform.paint(i, colour, area)
                : text.paints.item(i, colour, count)}
              onclick={() => {
                bench.select(i);
              }}
            >
              <span class="swatch" style:background={colour}></span>
              <span class="paint-name">
                <span class="mono hex">{colour.toUpperCase()}</span>
                {#if coat?.plate.ral}
                  <span class="note ral">
                    <span
                      class="swatch small"
                      aria-hidden="true"
                      style:background={coat.plate.ral.colour}
                    ></span>
                    {text.finishing.ral(coat.plate.ral.code)}
                  </span>
                {/if}
              </span>
              <span class="count">
                {coat
                  ? text.coating.number(coat.i + 1)
                  : freeform
                    ? text.freeform.area(area)
                    : num(count)}
              </span>
            </button>
            {#if bench.stencilsPossible}
              <input
                type="checkbox"
                checked={!bench.unstencilled.includes(colour)}
                aria-label={text.paints.stencil(i, colour)}
                onchange={() => {
                  bench.toggleStencil(colour);
                }}
              />
            {/if}
          </li>
        {/each}
      </ul>
      {#if bench.stencilsPossible}<p class="note">{text.paints.stencilNote}</p>{/if}

      {#if freeform}
        {#if freeform.plates.length > 1 || bench.settings.paintOrder.length}
          <div class="coat-actions" role="group" aria-label={text.coating.heading}>
            <button
              type="button"
              class="button"
              disabled={!bench.ready || coatPosition < 1}
              onclick={() => {
                bench.moveCoat(-1);
              }}
            >
              {text.coating.earlier}
            </button>
            <button
              type="button"
              class="button"
              disabled={!bench.ready ||
                coatPosition < 0 ||
                coatPosition + 1 >= freeform.plates.length}
              onclick={() => {
                bench.moveCoat(1);
              }}
            >
              {text.coating.later}
            </button>
            {#if bench.settings.paintOrder.length}
              <button
                type="button"
                class="button"
                onclick={() => {
                  bench.change('paintOrder', []);
                }}
              >
                {text.coating.reset}
              </button>
            {/if}
          </div>
        {/if}
        <Segmented
          label={text.finishing.palette}
          options={palettes}
          value={bench.settings.matchRal ? 'ral' : 'source'}
          onpick={(value) => {
            bench.change('matchRal', value === 'ral');
          }}
          stretch
        />
        {#if bench.settings.matchRal || bench.settings.mark === 'ral'}
          <p class="note">{text.finishing.ralNote}</p>
        {/if}
      {/if}

      {#if bench.colours.length > 1}
        <div class="merge">
          <p class="label">{text.paints.mergeInto(pickedColour)}</p>
          <div class="targets">
            {#each bench.colours as colour, i (i)}
              {#if i !== picked}
                <button
                  type="button"
                  class="target"
                  style:background={colour}
                  title={colour.toUpperCase()}
                  aria-label={text.paints.mergeTarget(picked, pickedColour, i, colour)}
                  onclick={() => {
                    bench.merge(pickedColour, colour);
                  }}
                ></button>
              {/if}
            {/each}
          </div>
        </div>
      {/if}

      {#if merges.length > 0}
        <div class="merge">
          <p class="label">{text.paints.merged}</p>
          <ul class="merged">
            {#each merges as [from, to], i (i)}
              <li>
                <span class="swatch small" style:background={from}></span>
                <span class="mono">{from.toUpperCase()}</span>
                <span class="into">{text.paints.into}</span>
                <span class="swatch small" style:background={to}></span>
                <span class="mono">{to.toUpperCase()}</span>
                <button
                  type="button"
                  class="remove"
                  aria-label={text.paints.unmerge(from, to)}
                  onclick={() => {
                    bench.unmerge(i);
                  }}
                >
                  <svg
                    width="12"
                    height="12"
                    viewBox="0 0 12 12"
                    stroke="currentColor"
                    stroke-width="1.5"
                    aria-hidden="true"
                  >
                    <path d="M2.5 2.5l7 7M9.5 2.5l-7 7" />
                  </svg>
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if freeform}
        {#if bench.selectedPlate}
          {@const plate = bench.selectedPlate}
          <div class="report">
            <p class="label">{text.freeform.plateCheck}</p>
            <p class="note">
              {text.freeform.plateFor(plate.paint, plate.colour)} · {text.freeform.openings(
                plate.openings
              )}
            </p>
            <PlateIssues {plate} />
          </div>
        {:else if bench.unstencilled.includes(pickedColour)}
          <p class="note">{text.freeform.nothingSelected}</p>
        {/if}
      {:else if bench.stencilsPossible && chart}
        {@const off = bench.unstencilled.includes(pickedColour)}
        <figure class="preview" class:off>
          <div class="measure" bind:clientWidth={previewWidth}>
            <TileCanvas
              {chart}
              colours={bench.colours}
              width={previewWidth}
              stencil={picked}
              {dark}
              label={text.stencil.label(
                picked,
                bench.colours[picked] ?? '',
                chart.counts[picked] ?? 0
              )}
            />
          </div>
          <figcaption class="note">
            {text.stencil.name(picked)} · {off
              ? text.stencil.none
              : text.stencil.holes(chart.counts[picked] ?? 0)}
          </figcaption>
        </figure>
      {/if}
    </section>
  {/if}

  <footer class="note">
    {#if bench.version}<p class="version">{text.engine(bench.version)}</p>{/if}
  </footer>
</div>

<style>
  .sidebar {
    min-height: 100%;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .heading {
    padding-bottom: 8px;
    border-bottom: 1px solid var(--line);
  }

  .split {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .paint-name {
    display: grid;
    gap: 2px;
  }

  .ral {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }

  .coat-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .coat-actions .button {
    padding-inline: 7px;
    min-height: 34px;
    font-size: 11px;
  }

  .aside {
    font-weight: 400;
    color: var(--text-3);
  }

  .thumbnail {
    display: block;
    width: 100%;
    max-height: 180px;
    object-fit: contain;
    background: var(--surface);
  }

  .file {
    display: flex;
    flex-direction: column;
  }

  .name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .paints {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .paints li {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--line);
  }

  .paints button {
    flex: 1;
    min-width: 0;
    min-height: 34px;
    padding: 0 8px;
    display: flex;
    align-items: center;
    gap: 10px;
    border: 0;
    background: transparent;
    font-size: 13px;
    text-align: left;
  }

  .paints input {
    width: 15px;
    height: 15px;
    margin: 0 8px 0 10px;
    flex: none;
    accent-color: var(--tint-text);
  }

  .off .hex,
  .off .count {
    color: var(--text-3);
  }

  .paints button:hover {
    background: var(--surface);
  }

  .paints button[aria-pressed='true'] {
    background: var(--tint);
    box-shadow: inset 2px 0 0 var(--tint-text);
  }

  .swatch {
    width: 18px;
    height: 18px;
    flex: none;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.2);
  }

  .swatch.small {
    width: 12px;
    height: 12px;
  }

  .merge {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .targets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .target {
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.2);
  }

  .target:hover,
  .target:focus-visible {
    outline: 2px solid var(--tint-text);
    outline-offset: 1px;
  }

  .merged {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 12px;
  }

  .merged li {
    min-height: 30px;
    display: flex;
    align-items: center;
    gap: 6px;
    border-bottom: 1px solid var(--line);
  }

  .into {
    color: var(--text-3);
  }

  .remove {
    margin-left: auto;
    width: 26px;
    height: 26px;
    padding: 0;
    display: grid;
    place-items: center;
    border: 0;
    background: transparent;
    color: var(--text-2);
  }

  .remove:hover {
    color: var(--text);
    background: var(--surface);
  }

  .hex {
    flex: 1;
  }

  .count {
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }

  .preview {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .preview.off .measure {
    opacity: 0.4;
  }

  .report {
    display: grid;
    gap: 6px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }

  footer {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .version {
    color: var(--text-3);
  }
</style>
