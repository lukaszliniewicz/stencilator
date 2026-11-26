<script lang="ts">
  // The right column: everything that shapes the chart and the stencils, grouped as the
  // work goes, with what the engine made of it underneath.
  import type { Settings } from './settings';
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
  </section>

  <section aria-labelledby="stencils-heading">
    <h2 id="stencils-heading" class="group">{text.settings.stencils}</h2>
    <div class="pair">
      <NumberField
        label={text.settings.thickness}
        value={s.thicknessMm}
        step={0.1}
        onchange={set('thicknessMm')}
      />
    </div>
  </section>

  <section aria-labelledby="file-heading">
    <h2 id="file-heading" class="group">{text.settings.chartFile}</h2>
    <div class="pair">
      <NumberField label={text.settings.dpi} value={s.dpi} step={50} onchange={set('dpi')} />
    </div>
  </section>
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
  }
</style>
