<script lang="ts">
  // The right column: the mode and the settings that shape its results.
  import type { Settings, SizeMode } from './settings';
  import ColourField from './ColourField.svelte';
  import NumberField from './NumberField.svelte';
  import Segmented from './Segmented.svelte';
  import { text } from './text';
  import type { Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
  }

  let { bench }: Props = $props();

  const s = $derived(bench.settings);
  const set =
    <K extends keyof Settings>(key: K) =>
    (value: Settings[K]): void => {
      bench.change(key, value);
    };
  const chart = $derived(bench.chart);
  const source = $derived(bench.source);
  const free = text.freeform;
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
  </section>

  {#if bench.isFreeform}
    <section aria-labelledby="size-heading">
      <h2 id="size-heading" class="group">{free.size}</h2>
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
    </section>

    <section aria-labelledby="detail-heading">
      <h2 id="detail-heading" class="group">{free.detail}</h2>
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
    </section>
  {:else}
    <section aria-labelledby="canvas-heading">
      <h2 id="canvas-heading" class="group">{text.settings.canvas}</h2>
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
        <p class="note">{text.settings.canvasNote}</p>
      {:else}
        <p class="note">
          {source
            ? text.settings.pixelNote(source.pixelsX, source.pixelsY)
            : text.settings.pixelNoteEmpty}
        </p>
      {/if}
      <ColourField
        label={text.settings.background}
        value={s.background}
        onchange={set('background')}
      />
    </section>

    <section aria-labelledby="grid-heading">
      <h2 id="grid-heading" class="group">{text.settings.grid}</h2>
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
      <ColourField
        label={text.settings.lineColour}
        value={s.gridColour}
        onchange={set('gridColour')}
      />
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
      {#if bench.isFreeform}
        <ColourField
          label={text.settings.background}
          value={s.background}
          onchange={set('background')}
        />
      {/if}
    </section>
  {/if}

  <section aria-labelledby="stencils-heading">
    <h2 id="stencils-heading" class="group">
      {bench.isFreeform ? free.plates : text.settings.stencils}
    </h2>
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
      <label class="bridge-option">
        <input
          type="checkbox"
          checked={s.bridgeWidthMm !== null}
          onchange={(event) => {
            bench.change('bridgeWidthMm', event.currentTarget.checked ? 1.2 : null);
          }}
        />
        {free.flatBridges}
      </label>
      {#if s.bridgeWidthMm !== null}
        <div class="pair">
          <NumberField
            label={free.bridgeWidth}
            value={s.bridgeWidthMm}
            step={0.1}
            onchange={set('bridgeWidthMm')}
          />
        </div>
        <p class="note">{free.bridgeNote}</p>
      {/if}
      <p class="label">{free.bed}</p>
      <div class="pair">
        <NumberField label={free.bedWidth} value={s.bedWidthMm} onchange={set('bedWidthMm')} />
        <NumberField label={free.bedDepth} value={s.bedDepthMm} onchange={set('bedDepthMm')} />
      </div>
    {/if}
  </section>

  {#if bench.isFreeform}
    <section aria-labelledby="checks-heading">
      <h2 id="checks-heading" class="group">{free.checks}</h2>
      <div class="pair">
        <NumberField label={free.minWeb} value={s.minWebMm} step={0.1} onchange={set('minWebMm')} />
        <NumberField
          label={free.minOpening}
          value={s.minOpeningMm}
          step={0.1}
          onchange={set('minOpeningMm')}
        />
      </div>
    </section>
  {:else}
    <section aria-labelledby="file-heading">
      <h2 id="file-heading" class="group">{text.settings.chartFile}</h2>
      <div class="pair">
        <NumberField label={text.settings.dpi} value={s.dpi} step={50} onchange={set('dpi')} />
      </div>
    </section>
  {/if}
</div>

<style>
  section {
    padding: 18px 16px;
    border-bottom: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 14px;
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

  .bridge-option {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .bridge-option input {
    accent-color: var(--tint-text);
  }
</style>
