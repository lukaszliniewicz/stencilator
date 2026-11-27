<script lang="ts">
  // The right column: the mode and the settings that shape its results.
  import type { Settings, SizeMode } from './settings';
  import ColourField from './ColourField.svelte';
  import NumberField from './NumberField.svelte';
  import Segmented from './Segmented.svelte';
  import SupportControls from './SupportControls.svelte';
  import { text } from './text';
  import type { Workbench } from './workbench.svelte';
  import type { Workspace } from './workspace.svelte';

  interface Props {
    bench: Workbench;
    workspace: Workspace;
    sizeHeading?: HTMLElement | undefined;
    changeBed: () => void;
    manageProfiles: () => void;
    placeSupport: () => void;
  }

  let {
    bench,
    workspace,
    sizeHeading = $bindable(),
    changeBed,
    manageProfiles,
    placeSupport
  }: Props = $props();

  const s = $derived(bench.settings);
  const set =
    <K extends keyof Settings>(key: K) =>
    (value: Settings[K]): void => {
      bench.change(key, value);
    };
  const chart = $derived(bench.chart);
  const result = $derived(bench.isFreeform ? bench.freeform : chart);
  const source = $derived(bench.source);
  const free = text.freeform;
  const marks = (['none', 'number', 'ral'] as const).map((value) => ({
    value,
    label: text.finishing.marks[value]
  }));
  const projectModes = $derived([
    {
      value: 'tiles' as const,
      label: text.project.modes.tiles,
      disabled: !!bench.artwork && !source
    },
    { value: 'freeform' as const, label: text.project.modes.freeform }
  ]);
  const sizes = $derived(
    (['natural', 'width', 'height', 'within'] as const).map((value: SizeMode) => ({
      value,
      label: free.sizes[value],
      disabled: value === 'natural' && !!source
    }))
  );
  const showPaintReduction = $derived(!bench.typed || s.mode !== 'colour' || s.colours < 2);
  const SIZINGS = (['pixels', 'canvas'] as const).map((value) => ({
    value,
    label: text.settings.sizings[value]
  }));
  const MODES = (['colour', 'greyscale', 'black-white'] as const).map((value) => ({
    value,
    label: text.settings.modes[value]
  }));
</script>

{#snippet help(label: string, body: string)}
  <details class="help">
    <summary aria-label={text.help.about(label)}>?</summary>
    <p class="note">{body}</p>
  </details>
{/snippet}

<div class="panel">
  <section class="mode" aria-labelledby="project-mode-heading">
    <h2 id="project-mode-heading" class="group">{text.project.mode}</h2>
    <Segmented
      label={text.project.mode}
      options={projectModes}
      value={s.projectMode}
      onpick={bench.chooseMode}
      stretch
    />
    <p class="note">
      {bench.artwork && !source
        ? text.project.tilesNeedPicture
        : bench.isFreeform
          ? text.project.freeformNote
          : text.project.tilesNote}
    </p>
    {@render help(text.project.mode, text.help.artwork)}
  </section>

  {#if bench.isFreeform}
    <section aria-labelledby="size-heading">
      <h2 id="size-heading" class="group" tabindex="-1" bind:this={sizeHeading}>{free.size}</h2>
      <Segmented
        label={free.size}
        options={sizes}
        value={s.sizeMode}
        onpick={set('sizeMode')}
        stretch
      />
      {#if s.sizeMode === 'natural'}
        <p class="note">{free.naturalSize}</p>
      {:else}
        <div class="pair">
          {#if s.sizeMode !== 'height'}
            <NumberField
              label={text.settings.width}
              value={s.artWidthMm}
              onchange={set('artWidthMm')}
            />
          {/if}
          {#if s.sizeMode !== 'width'}
            <NumberField
              label={text.settings.height}
              value={s.artHeightMm}
              onchange={set('artHeightMm')}
            />
          {/if}
        </div>
      {/if}
      {#if !showPaintReduction}
        <ColourField
          label={text.settings.background}
          value={s.background}
          onchange={set('background')}
        />
      {/if}
      {#if bench.current && bench.valid && result}
        <p class="note">{text.help.resulting(result.widthMm, result.heightMm)}</p>
      {/if}
    </section>
  {:else}
    <section aria-labelledby="size-heading">
      <h2 id="size-heading" class="group" tabindex="-1" bind:this={sizeHeading}>
        {text.help.sizing}
      </h2>
      <Segmented
        label={text.settings.sizing}
        options={SIZINGS}
        value={s.sizing}
        stretch
        onpick={set('sizing')}
      />
      {#if s.sizing === 'canvas'}
        <div class="pair">
          <NumberField
            label={text.settings.width}
            value={s.canvasWidthMm}
            onchange={set('canvasWidthMm')}
          />
          <NumberField
            label={text.settings.height}
            value={s.canvasHeightMm}
            onchange={set('canvasHeightMm')}
          />
        </div>
      {:else}
        <p class="note">
          {source
            ? text.settings.pixelNote(source.pixelsX, source.pixelsY)
            : text.settings.pixelNoteEmpty}
        </p>
      {/if}
      {@render help(text.help.sizing, s.sizing === 'pixels' ? text.help.pixels : text.help.canvas)}
      <div class="pair">
        <NumberField
          label={text.settings.tile}
          value={s.tileMm}
          step={0.5}
          onchange={set('tileMm')}
        />
        <NumberField
          label={text.settings.line}
          value={s.lineMm}
          step={0.1}
          onchange={set('lineMm')}
        />
      </div>
      {@render help(text.help.tileDetails, text.help.web)}
      {#if bench.current && bench.valid && result}
        <p class="note">{text.help.resulting(result.widthMm, result.heightMm)}</p>
      {/if}
      {#if chart}
        <p class="note">{text.settings.rounded(Math.round(s.dpi), chart.tileMm, chart.lineMm)}</p>
        {#if chart.linePx === 0}
          <p class="notice">{text.settings.noLines}</p>
        {:else if chart.lineMm < bench.minWebMm}
          <p class="notice">{text.settings.thinLines(bench.minWebMm)}</p>
        {/if}
      {/if}
    </section>
  {/if}

  {#if showPaintReduction}
    <section aria-labelledby="paints-settings-heading">
      <h2 id="paints-settings-heading" class="group">{text.settings.paints}</h2>
      <Segmented
        label={text.settings.mode}
        options={MODES}
        value={s.mode}
        stretch
        onpick={set('mode')}
      />
      <div class="pair">
        <NumberField
          label={text.settings.colours}
          value={s.colours}
          disabled={s.mode === 'black-white'}
          onchange={set('colours')}
        />
      </div>
      {#if s.mode === 'black-white'}<p class="note">{text.settings.twoPaints}</p>{/if}
      <ColourField
        label={text.settings.background}
        value={s.background}
        onchange={set('background')}
      />
    </section>
  {/if}

  <section aria-labelledby="printer-heading">
    <h2 id="printer-heading" class="group">{text.printer.title}</h2>
    <p>
      <strong>{workspace.activeProfile?.name ?? text.printer.custom}</strong> ·
      <span class="mono">{text.bed.size(s.bedWidthMm, s.bedDepthMm)}</span>
    </p>
    {#if workspace.profileUpdate}<p class="notice">{workspace.profileUpdate}</p>{/if}
    <div class="button-row">
      <button type="button" class="quiet" onclick={changeBed}>{text.printer.change}</button>
      <button type="button" class="quiet" onclick={manageProfiles}>{text.printer.manage}</button>
    </div>
  </section>

  {#if bench.isFreeform}
    <section aria-labelledby="supports-heading">
      <h2 id="supports-heading" class="group">{text.supports.heading}</h2>
      {@render help(text.supports.heading, text.help.supports)}
      <SupportControls {bench} {placeSupport} />
    </section>
  {/if}

  <details class="section">
    <summary class="group">
      {bench.isFreeform ? text.help.plateDetails : text.settings.stencils}
    </summary>
    <div class="pair">
      <NumberField
        label={text.settings.thickness}
        value={s.thicknessMm}
        step={0.1}
        onchange={set('thicknessMm')}
      />
      {#if bench.isFreeform}
        <NumberField label={free.margin} value={s.marginMm} step={0.5} onchange={set('marginMm')} />
      {/if}
    </div>
    {#if bench.isFreeform}
      <label class="check-option">
        <input
          type="checkbox"
          checked={s.registrationClearanceMm !== null}
          onchange={(event) => {
            bench.change('registrationClearanceMm', event.currentTarget.checked ? 0.3 : null);
          }}
        />
        {text.registration.enable}
      </label>
      {@render help(text.registration.heading, text.help.frame)}
      {#if s.registrationClearanceMm !== null}
        <div class="pair">
          <NumberField
            label={text.registration.clearance}
            value={s.registrationClearanceMm}
            step={0.05}
            onchange={set('registrationClearanceMm')}
          />
        </div>
        <p class="note">{text.registration.fitNote}</p>
      {/if}
    {/if}
  </details>

  {#if bench.isFreeform}
    <details class="section">
      <summary class="group">{text.help.finishDetails}</summary>
      <h2 class="label">{free.detail}</h2>
      <div class="pair">
        <NumberField label={free.cell} value={s.cellMm} step={0.01} onchange={set('cellMm')} />
        <NumberField
          label={free.patch}
          value={s.minPatchMm2}
          step={0.1}
          onchange={set('minPatchMm2')}
        />
      </div>
      <p class="note">{free.detailNote}</p>
      <h2 class="label">{free.checks}</h2>
      <div class="pair">
        <NumberField label={free.minWeb} value={s.minWebMm} step={0.1} onchange={set('minWebMm')} />
        <NumberField
          label={free.minOpening}
          value={s.minOpeningMm}
          step={0.1}
          onchange={set('minOpeningMm')}
        />
      </div>
      <h2 class="label">{text.finishing.heading}</h2>
      <Segmented
        label={text.finishing.mark}
        options={marks}
        value={s.mark}
        onpick={set('mark')}
        stretch
      />
      <div class="pair">
        <NumberField
          label={text.finishing.chamfer}
          step={0.1}
          value={s.chamferMm}
          onchange={set('chamferMm')}
        />
        <NumberField
          label={text.finishing.overlap}
          step={0.1}
          value={s.overlapMm}
          onchange={set('overlapMm')}
        />
      </div>
      <p class="note">{text.finishing.note}</p>
    </details>
  {:else}
    <details class="section">
      <summary class="group">{text.help.chartDetails}</summary>
      <div class="pair">
        <NumberField label={text.settings.dpi} value={s.dpi} step={50} onchange={set('dpi')} />
      </div>
      <ColourField
        label={text.settings.lineColour}
        value={s.gridColour}
        onchange={set('gridColour')}
      />
    </details>
  {/if}
</div>

<style>
  section,
  .section {
    padding: 18px 16px;
    border-bottom: 1px solid var(--line);
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .section[open] > :not(summary) {
    margin-top: 12px;
  }
  summary {
    cursor: pointer;
  }
  .help summary {
    display: inline-grid;
    place-items: center;
    min-width: 26px;
    min-height: 26px;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    color: var(--text-2);
    font-weight: 600;
  }
  .help p {
    margin-top: 7px;
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    align-items: end;
  }

  .mode {
    gap: 8px;
  }

  .check-option {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .check-option input {
    accent-color: var(--tint-text);
  }
</style>
