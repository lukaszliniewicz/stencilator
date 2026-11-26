<script lang="ts">
  import ColourField from './ColourField.svelte';
  import NumberField from './NumberField.svelte';
  import Raster from './Raster.svelte';
  import { text } from './text';
  import type { Workbench } from './workbench.svelte';

  let { bench, choose }: { bench: Workbench; choose: () => void } = $props();
  const words = text.freeform;
  const STENCIL = 'AllertaStencil-Regular.ttf';
  const DEFAULT = 'NotoSans-Bold.ttf';
  const CUSTOM = 'custom';
  let fontPicker: HTMLInputElement | undefined = $state();

  const edited = $derived(bench.typed);
  const original = $derived(bench.freeform?.artwork);
  const font = $derived(bench.fonts.files[0]);
  const choice = $derived(font?.data ? CUSTOM : (font?.name ?? DEFAULT));
  const tip = $derived(
    choice === DEFAULT && bench.freeform?.plates.some((plate) => plate.islands.length > 0)
  );

  function openFont(event: Event): void {
    const picker = event.currentTarget as HTMLInputElement;
    const file = picker.files?.[0];
    picker.value = '';
    if (file) bench.addFont(file);
  }
</script>

<input
  type="file"
  accept=".ttf,.otf,.ttc"
  aria-label={words.fontLabel}
  bind:this={fontPicker}
  onchange={openFont}
  hidden
/>

{#if edited}
  <label class="field">
    <span class="label">{words.typed}</span>
    <textarea
      rows="3"
      value={edited.value}
      oninput={(event) => {
        bench.changeText('value', event.currentTarget.value);
      }}></textarea>
  </label>
  <label class="field">
    <span class="label">{words.font}</span>
    <select
      value={choice}
      onchange={(event) => {
        bench.builtinFont(event.currentTarget.value);
      }}
    >
      {#each Object.entries(words.fonts) as [name, label] (name)}
        <option value={name}>{label}</option>
      {/each}
      {#if font?.data}<option value={CUSTOM} disabled>{font.name}</option>{/if}
    </select>
  </label>
  <div class="pair">
    <NumberField
      label={words.fontSize}
      value={edited.fontMm}
      step={0.5}
      onchange={(value: number) => {
        bench.changeText('fontMm', value);
      }}
    />
    <ColourField
      label={words.textColour}
      value={edited.colour}
      stacked
      onchange={(value: string) => {
        bench.changeText('colour', value);
      }}
    />
  </div>
  {#if tip}
    <div class="notice">
      <p>{words.stencilFontTip}</p>
      <button
        type="button"
        class="link"
        onclick={() => {
          bench.builtinFont(STENCIL);
        }}
      >
        {words.tryStencilFont}
      </button>
    </div>
  {/if}
{:else if bench.artwork}
  {#if bench.pictureUrl}
    <img class="thumbnail" src={bench.pictureUrl} alt={text.picture.alt} />
  {:else if original}
    <div
      class="svg-thumb"
      style:aspect-ratio={original.width / original.height}
      style:background={bench.settings.background}
    >
      <Raster preview={original} label={words.artworkAlt} />
    </div>
  {/if}
  <p class="file">{bench.fileName}</p>
{:else}
  <p class="note">{words.none}</p>
{/if}

<div class="actions">
  <button type="button" class="button" disabled={!bench.canOpen} onclick={choose}>
    {bench.artwork ? text.picture.replace : words.open}
  </button>
  {#if bench.artwork && !edited}
    <button type="button" class="button" disabled={!bench.canOpen} onclick={bench.typeText}>
      {words.type}
    </button>
  {/if}
  {#if edited !== undefined || bench.artwork?.kind === 'svg'}
    <button
      type="button"
      class="button"
      disabled={bench.opening}
      onclick={() => fontPicker?.click()}
    >
      {words.addFont}
    </button>
  {/if}
</div>

{#if bench.artwork?.kind === 'svg' && bench.fonts.files.some((file) => file.data)}
  <ul class="font-list note">
    {#each bench.fonts.files as file, i (i)}
      {#if file.data}<li>{file.name}</li>{/if}
    {/each}
  </ul>
  <button type="button" class="link" onclick={bench.clearFonts}>{words.clearFonts}</button>
{/if}

<style>
  .field {
    min-width: 0;
    display: grid;
    gap: 6px;
  }

  textarea,
  select {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 0;
    background: var(--field);
    color: var(--text);
    font: inherit;
  }

  textarea {
    resize: vertical;
  }

  select {
    min-height: 40px;
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    align-items: end;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .actions .button {
    min-height: 34px;
    padding: 0 10px;
    font-size: 11px;
  }

  .thumbnail,
  .svg-thumb {
    max-height: 146px;
    max-width: 100%;
    margin-inline: auto;
    display: block;
    object-fit: contain;
  }

  .svg-thumb {
    height: 130px;
  }

  .file {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .font-list {
    margin: 0;
    padding-left: 16px;
    overflow-wrap: anywhere;
  }

  .link {
    width: fit-content;
    padding: 0;
    border: 0;
    background: none;
    color: var(--tint-text);
    font-size: 12px;
    text-align: left;
    text-decoration: underline;
  }

  .notice .link {
    margin-top: 4px;
  }
</style>
