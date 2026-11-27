<script lang="ts">
  import PlateIssues from './PlateIssues.svelte';
  import { footprint, printParts } from './print-parts';
  import Segmented from './Segmented.svelte';
  import { text } from './text';
  import Viewport from './Viewport.svelte';
  import type { Workbench } from './workbench.svelte';

  let {
    bench,
    changeBed,
    changeSize
  }: {
    bench: Workbench;
    changeBed: () => void;
    changeSize: () => void;
  } = $props();
  const words = text.bed;
  const id = $props.id();
  const angles = (['angled', 'top'] as const).map((value) => ({
    value,
    label: words.angles[value]
  }));
  let angle = $state<'angled' | 'top'>('angled');
  let frame = $state<'bed' | 'part'>('bed');
  let fitted = $state(true);
  let resetCount = $state(0);
  let room: DOMRectReadOnly | undefined = $state();
  const parts = $derived(printParts(bench));
  const part = $derived(
    parts?.find(({ id }) => id === (bench.bedPart ?? bench.selected)) ?? parts?.[0]
  );
  const bedW = $derived(bench.settings.bedWidthMm);
  const bedD = $derived(bench.settings.bedDepthMm);
  const turned = $derived(part?.fit === 'quarter-turn');
  const over = $derived(part?.fit === 'too-large');
  const width = $derived((turned ? part?.depth : part?.width) ?? 0);
  const depth = $derived((turned ? part?.width : part?.depth) ?? 0);
  const offsetX = $derived((bedW - width) / 2);
  const offsetY = $derived((bedD - depth) / 2);
  const space = $derived({ x: bedW - width, y: bedD - depth });
  let shape = $state.raw<{ part: NonNullable<typeof part>; path: ReturnType<typeof footprint> }>();
  let failed = $state(false);
  const visible = $derived(shape?.part === part ? shape?.path : undefined);
  const across = 0.866;
  const down = 0.44;
  const projection = `matrix(${[across, down, -across, down, 0, 0].join(' ')})`;

  function project(x: number, y: number): [number, number] {
    return angle === 'angled' ? [across * (x - y), down * (x + y)] : [x, y];
  }

  const world = $derived.by(() => {
    const [x, y, w, d]: [number, number, number, number] =
      frame === 'part' && part ? [offsetX, offsetY, width, depth] : [0, 0, bedW, bedD];
    const corners = [project(x, y), project(x + w, y), project(x, y + d), project(x + w, y + d)];
    const xs = corners.map(([value]) => value);
    const ys = corners.map(([, value]) => value);
    const left = Math.min(...xs);
    const top = Math.min(...ys);
    const wProjected = Math.max(...xs) - left;
    const hProjected = Math.max(...ys) - top;
    const margin = frame === 'bed' ? 0.14 : 0.07;
    const padX = wProjected * margin;
    const padY = hProjected * margin;
    return [left - padX, top - padY, wProjected + 2 * padX, hProjected + 2 * padY].join(' ');
  });
  const side = $derived.by(() => {
    const top = [project(bedW, 0), project(bedW, bedD), project(0, bedD)];
    const bottom = [...top].reverse().map(([x, y]) => [x, y + 3]);
    return [...top, ...bottom].map((point) => point.join(',')).join(' ');
  });
  const placement = $derived(
    `translate(${[turned ? offsetX + width : offsetX, offsetY].join(' ')})${turned ? ' rotate(90)' : ''}`
  );

  $effect(() => {
    const selected = part;
    if (!selected || !bench.ready || shape?.part === selected) return;
    let active = true;
    failed = false;
    void bench
      .bedMesh(selected.id)
      .then((file) => {
        if (!active) return;
        if (file) shape = { part: selected, path: footprint(file) };
        else failed = true;
      })
      .catch(() => {
        if (active) failed = true;
      });
    return () => {
      active = false;
    };
  });

  function fit(to: 'bed' | 'part'): void {
    frame = to;
    resetCount++;
  }
</script>

<div class="bed quiet-scroll">
  {#if part && parts}
    <div class="print-head">
      <label class="dialog-field">
        <span class="label">{words.partSelect}</span>
        <select
          class="field-input"
          value={String(part.id)}
          onchange={(event) => {
            const value = event.currentTarget.value;
            bench.showBedPart(value === 'frame' ? value : Number(value));
          }}
        >
          {#each parts as item (item.id)}
            <option value={String(item.id)}>{item.name}</option>
          {/each}
        </select>
      </label>
      <Segmented
        label={words.orientation}
        options={angles}
        value={angle}
        onpick={(value: 'angled' | 'top') => {
          angle = value;
        }}
      />
    </div>

    <section class="summary" aria-label={words.findings}>
      <div class="verdict" aria-live="polite">
        <p class="result" class:over>{words.fit[part.fit]}</p>
        {#if turned}<p class="note">{words.rotated}</p>{/if}
        {#if over}
          <p class="note">{words.overhang}</p>
          <p class="note">
            {words.asDrawnOver(words.overflow(-space.x, -space.y))}
            {words.turnedOver(words.overflow(part.depth - bedW, part.width - bedD))}
          </p>
        {:else if turned || Math.min(space.x, space.y) < 0.1}
          <p class="note">{words.clearance(space.x, space.y)}</p>
        {/if}
      </div>
      <dl>
        <div>
          <dt>{words.part}</dt>
          <dd>{words.size(part.width, part.depth)} · {words.height(part.height)}</dd>
        </div>
        <div>
          <dt>{words.title}</dt>
          <dd>{words.size(bedW, bedD)}</dd>
        </div>
      </dl>
      <div class="button-row">
        {#if over}<button class="quiet" type="button" onclick={changeSize}>
            {words.resize}
          </button>{/if}
        <button class="quiet" type="button" onclick={changeBed}>{text.printer.change}</button>
      </div>
    </section>

    <Viewport
      bind:room
      label={words.title}
      frameKey={[part.id, world, resetCount].join('-')}
      onmanual={() => {
        fitted = false;
      }}
      onreset={() => {
        fitted = true;
      }}
    >
      {#if visible}
        <svg
          viewBox={world}
          role="img"
          width={room?.width ?? 0}
          height={room?.height ?? 0}
          aria-label={words.drawing(part.name, bedW, bedD, words.fit[part.fit])}
        >
          <defs>
            <pattern id={`${id}-grid`} width="10" height="10" patternUnits="userSpaceOnUse">
              <path
                class="grid-line"
                d="M10 0H0V10"
                fill="none"
                stroke-width="0.6"
                vector-effect="non-scaling-stroke"
              />
            </pattern>
            <clipPath id={`${id}-clip`}><rect width={bedW} height={bedD} /></clipPath>
            <path
              id={`${id}-part`}
              d={visible.path}
              transform={`translate(${[visible.x, visible.y].join(' ')})`}
            />
          </defs>
          {#if angle === 'angled'}
            <polygon class="side" points={side} />
          {/if}
          <g transform={angle === 'angled' ? projection : undefined}>
            <rect class="surface" width={bedW} height={bedD} />
            <rect fill={`url(#${id}-grid)`} width={bedW} height={bedD} />
            {#if over}
              <use class="overhang" href={`#${id}-part`} transform={placement} />
            {/if}
            <g clip-path={over ? `url(#${id}-clip)` : undefined}>
              <use class="on-bed" href={`#${id}-part`} transform={placement} />
            </g>
          </g>
        </svg>
      {:else}
        <p class="note" role="status">{failed ? words.unavailable : text.printPreview.loading}</p>
      {/if}
    </Viewport>

    <div class="fit-controls button-row" role="group" aria-label={words.frame}>
      <button
        type="button"
        class="button"
        aria-pressed={fitted && frame === 'bed'}
        onclick={() => {
          fit('bed');
        }}
      >
        {words.fitBed}
      </button>
      <button
        type="button"
        class="button"
        aria-pressed={fitted && frame === 'part'}
        onclick={() => {
          fit('part');
        }}
      >
        {words.fitPart}
      </button>
      <p class="note">{words.footprint}</p>
    </div>
    <div class="notes">
      <p class="note">{words.fitNote}</p>
      {#if part.plate}
        <PlateIssues plate={part.plate} includeFit={false} />
        {#if part.plate.raised_connectors?.count}<p class="note">{text.supports.trial}</p>{/if}
      {:else if !bench.isFreeform && bench.chart && bench.chart.lineMm < bench.minWebMm}
        <p class="notice">{text.settings.thinLines(bench.minWebMm)}</p>
      {/if}
    </div>
  {:else if parts === null}
    <section class="summary" aria-label={words.findings}>
      <p class="notice" role="status">{words.invalidBed}</p>
      <button class="quiet" type="button" onclick={changeBed}>{text.printer.change}</button>
    </section>
  {:else}
    <p class="notice" role="status">
      {bench.current && bench.valid
        ? words.none
        : bench.current
          ? words.unavailable
          : text.printPreview.loading}
    </p>
  {/if}
</div>

<style>
  .bed {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    background: var(--surface) var(--canvas-grid);
    background-size: 20px 20px;
  }

  .bed > :global(.viewport) {
    min-height: 180px;
  }

  .print-head,
  .summary,
  .fit-controls,
  .notes {
    position: relative;
    flex: none;
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }

  .print-head,
  .summary {
    display: flex;
    align-items: end;
    flex-wrap: wrap;
    gap: 6px 14px;
  }

  .print-head label {
    flex: 1 1 180px;
  }
  .summary {
    align-items: center;
  }
  .verdict {
    flex: 1 1 180px;
  }
  .result {
    font-weight: 700;
    color: var(--tint-text);
  }
  .result.over {
    color: var(--warn-text);
  }
  dl {
    margin: 0;
    font-size: 12px;
  }
  dl div {
    display: flex;
    gap: 6px;
    align-items: baseline;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
    font-family: var(--mono);
  }
  .fit-controls .button {
    min-height: 32px;
    padding-inline: 8px;
    font-size: 10px;
  }
  .fit-controls .button[aria-pressed='true'] {
    color: var(--tint-text);
    background: var(--tint);
  }
  .notes {
    display: grid;
    gap: 5px;
  }
  svg {
    display: block;
  }
  .side {
    fill: light-dark(#798c99, #293641);
  }
  .surface {
    fill: light-dark(#d7e0e5, #354452);
    stroke: light-dark(#536570, #8ca1b1);
    stroke-width: 0.45;
  }
  .grid-line {
    stroke: light-dark(#acbac4, #536678);
  }
  .on-bed {
    fill: light-dark(#3e617f, #a6c3da);
  }
  .overhang {
    fill: light-dark(#bd6835, #f0ab75);
  }
</style>
