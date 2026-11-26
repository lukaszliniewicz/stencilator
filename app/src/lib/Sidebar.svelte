<script lang="ts">
  // The left column: the name, the theme, the picture, and once there is a chart, its
  // paints: which get stencils, merging the picked one into another, and its stencil.
  import TileCanvas from './TileCanvas.svelte';
  import { num, text } from './text';
  import { THEMES, type Theme } from './theme.svelte';
  import type { Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
    theme: Theme;
    choose: () => void;
  }

  let { bench, theme, choose }: Props = $props();

  let previewWidth = $state(0);
  const chart = $derived(bench.chart);
  const picked = $derived(bench.selected);
  const pickedColour = $derived(bench.colours[picked] ?? '');
  const merges = $derived(bench.settings.merges);
</script>

<div class="sidebar">
  <header class="brand">
    <p class="wordmark">
      <svg width="30" height="30" viewBox="0 0 34 34" aria-hidden="true">
        <rect class="plate" x="1" y="1" width="32" height="32" />
        <rect class="hole" x="6" y="6" width="9" height="9" />
        <rect class="hole" x="19" y="19" width="9" height="9" />
        <rect class="hole accent" x="19" y="6" width="9" height="9" />
      </svg>
      <span>{text.wordmark}.</span>
    </p>
    <div class="themes" role="group" aria-label={text.theme.label}>
      {#each THEMES as choice (choice)}
        <button
          type="button"
          aria-pressed={theme.choice === choice}
          onclick={() => {
            theme.pick(choice);
          }}
        >
          {text.theme[choice]}
        </button>
      {/each}
    </div>
  </header>

  <section aria-labelledby="picture-heading">
    <h2 id="picture-heading" class="label heading">{text.picture.heading}</h2>
    {#if bench.source}
      <img class="thumbnail" src={bench.pictureUrl} alt={text.picture.alt} />
      <p class="file">
        <span class="name">{bench.fileName}</span>
        <span class="note">{text.picture.pixels(bench.source.width, bench.source.height)}</span>
      </p>
      <button type="button" class="button" onclick={choose}>{text.picture.replace}</button>
    {:else}
      <p class="none">{text.picture.none}</p>
      <button type="button" class="button" onclick={choose}>
        <svg
          width="16"
          height="16"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          aria-hidden="true"
        >
          <path d="M8 11V2.5M4.5 6L8 2.5 11.5 6" />
          <path d="M2.5 10.5v3h11v-3" />
        </svg>
        {text.picture.open}
      </button>
    {/if}
  </section>

  {#if chart}
    <section aria-labelledby="paints-heading">
      <h2 id="paints-heading" class="label heading split">
        <span>{text.paints.heading}</span>
        <span class="aside">{text.paints.order(chart.counts.length)}</span>
      </h2>
      <ul class="paints">
        {#each bench.colours as colour, i (i)}
          <li class:off={bench.unstencilled.includes(colour)}>
            <button
              type="button"
              aria-pressed={picked === i}
              aria-label={text.paints.item(i, colour, chart.counts[i] ?? 0)}
              onclick={() => {
                bench.select(i);
              }}
            >
              <span class="swatch" style:background={colour}></span>
              <span class="mono hex">{colour.toUpperCase()}</span>
              <span class="count">{num(chart.counts[i] ?? 0)}</span>
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

      {#if bench.stencilsPossible}
        {@const off = bench.unstencilled.includes(pickedColour)}
        <figure class="preview" class:off>
          <div class="measure" bind:clientWidth={previewWidth}>
            <TileCanvas
              {chart}
              colours={bench.colours}
              width={previewWidth}
              stencil={picked}
              dark={theme.dark}
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
  {:else}
    <section aria-labelledby="how-heading">
      <h2 id="how-heading" class="label heading">{text.how.heading}</h2>
      <ol class="steps">
        {#each text.how.steps as step, i (step.title)}
          <li>
            <span class="number">{i + 1}</span>
            <span><strong>{step.title}</strong> <span class="body">{step.body}</span></span>
          </li>
        {/each}
      </ol>
    </section>
  {/if}

  <footer class="note">
    <p>{text.privacy}</p>
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

  .brand {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .wordmark {
    display: flex;
    align-items: center;
    gap: 12px;
    font-family: var(--serif);
    font-size: 30px;
    letter-spacing: -0.02em;
    line-height: 1;
  }

  .plate {
    fill: var(--text);
  }

  .hole {
    fill: var(--panel);
  }

  .accent {
    fill: var(--tint-text);
  }

  .themes {
    display: flex;
    gap: 12px;
  }

  .themes button {
    padding: 4px 0;
    border: 0;
    background: transparent;
    color: var(--text-3);
    font-size: 10px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .themes button:hover {
    color: var(--text);
  }

  .themes button[aria-pressed='true'] {
    color: var(--text);
    font-weight: 700;
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

  .aside {
    font-weight: 400;
    color: var(--text-3);
  }

  .none {
    padding: 20px 12px;
    border: 1px dashed var(--line-strong);
    color: var(--text-2);
    font-size: 12px;
    text-align: center;
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

  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 14px;
    font-size: 13px;
  }

  .steps li {
    display: flex;
    gap: 12px;
  }

  .number {
    font-weight: 700;
    color: var(--tint-text);
  }

  .steps strong {
    font-weight: 600;
  }

  .body {
    color: var(--text-2);
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
