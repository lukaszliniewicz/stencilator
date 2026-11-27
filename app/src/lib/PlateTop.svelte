<script lang="ts">
  import type { PlateDrawing, PlateReport, Point, SupportDrawing } from '../engine/client';
  import { text } from './text';
  import type { Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
    drawing: PlateDrawing;
    plate: PlateReport;
    islands: boolean;
    webs: boolean;
    openings: boolean;
    width: number;
  }

  let { bench, drawing, plate, islands, webs, openings, width }: Props = $props();
  const millimetres = (point: Point): Point => [
    point[0] / 1000,
    drawing.height_mm - point[1] / 1000
  ];
  const draft = $derived(bench.supportDraft);
  const labelSize = $derived((11 * drawing.width_mm) / Math.max(1, width));
  const placedEnds = $derived(draft?.ends.filter((point) => point.every(Number.isFinite)) ?? []);

  function place(event: MouseEvent & { currentTarget: SVGRectElement }): void {
    const matrix = event.currentTarget.getScreenCTM();
    if (!matrix) return;
    const { x, y } = new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix.inverse());
    bench.setSupportEnd([Math.round(x * 1000), Math.round((drawing.height_mm - y) * 1000)]);
  }

  function keySelect(event: KeyboardEvent, support: SupportDrawing): void {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      bench.selectSupport(support);
    }
  }
</script>

<svg
  class="plate"
  viewBox="0 0 {drawing.width_mm} {drawing.height_mm}"
  role={drawing.supports?.length ? 'group' : 'img'}
  aria-label={text.freeform.plateFor(plate.paint, plate.colour)}
>
  <path class="material" d={drawing.plate} fill-rule="evenodd" />
  {#if drawing.chamfer}<path class="chamfer" d={drawing.chamfer} fill-rule="evenodd" />{/if}
  {#if drawing.mark}<path class="mark" d={drawing.mark} fill-rule="evenodd" />{/if}
  {#if islands}<path class="islands" d={drawing.islands} fill-rule="evenodd" />{/if}
  {#if webs}<path class="webs" d={drawing.thin_webs} fill-rule="evenodd" />{/if}
  {#if openings}<path class="openings" d={drawing.narrow_openings} fill-rule="evenodd" />{/if}

  {#each drawing.supports ?? [] as support, i (support)}
    {@const [a, b] = support.ends.map(millimetres) as [Point, Point]}
    <g
      role="button"
      tabindex={bench.ready && !draft ? 0 : -1}
      aria-label={text.supports.item(i, bench.supportKind === 'raised')}
      aria-pressed={bench.selectedSupport === support}
      aria-disabled={!bench.ready || !!draft}
      onclick={() => {
        bench.selectSupport(support);
      }}
      onkeydown={(event) => {
        keySelect(event, support);
      }}
      class="support"
      class:raised={bench.supportKind === 'raised'}
      class:selected={bench.selectedSupport === support}
    >
      {#if support.issue}
        <line class="invalid" x1={a[0]} y1={a[1]} x2={b[0]} y2={b[1]} />
      {:else}
        <path class="footprint" d={support.path} />
      {/if}
      <line class="hit" x1={a[0]} y1={a[1]} x2={b[0]} y2={b[1]} />
      <text
        x={(a[0] + b[0]) / 2}
        y={(a[1] + b[1]) / 2}
        font-size={labelSize}
        text-anchor="middle"
        dominant-baseline="central"
        aria-hidden="true"
      >
        {i + 1}
      </text>
    </g>
  {/each}

  {#if draft}
    {#if placedEnds.length === 2}
      {@const [a, b] = placedEnds.map(millimetres) as [Point, Point]}
      <line class="draft" x1={a[0]} y1={a[1]} x2={b[0]} y2={b[1]} />
    {/if}
    {#each placedEnds as point (point)}
      {@const [x, y] = millimetres(point)}
      <circle class="end" cx={x} cy={y} r={labelSize * 0.45} />
    {/each}
    <rect
      class="target"
      role="presentation"
      width={drawing.width_mm}
      height={drawing.height_mm}
      onclick={place}
    />
  {/if}
</svg>

<style>
  .plate {
    display: block;
    width: 100%;
    height: 100%;
  }

  .material {
    fill: light-dark(#404040, #7a7a7a);
  }

  .chamfer {
    fill: light-dark(#a7a7a7, #474747);
  }

  .mark {
    fill: light-dark(#c8c8c8, #2d2d2d);
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

  .support {
    cursor: pointer;
  }

  .support text {
    fill: white;
    stroke: #172554;
    stroke-width: 3px;
    paint-order: stroke;
    vector-effect: non-scaling-stroke;
    font-weight: 700;
    pointer-events: none;
  }

  .footprint {
    fill: light-dark(#888888, #525252);
    stroke: var(--tint-text);
    stroke-width: 1px;
    vector-effect: non-scaling-stroke;
  }

  .raised .footprint {
    fill: var(--tint-text);
  }

  .support:hover .footprint,
  .support:focus-visible .footprint,
  .selected .footprint {
    fill: light-dark(#93c5fd, #1e3a8a);
    stroke-width: 2px;
  }

  .invalid,
  .draft {
    stroke: var(--tint-text);
    vector-effect: non-scaling-stroke;
    stroke-width: 2px;
    stroke-dasharray: 4px 3px;
  }

  .invalid {
    stroke: light-dark(#a16207, #fbbf24);
  }

  .hit {
    stroke: transparent;
    stroke-width: 14px;
    vector-effect: non-scaling-stroke;
  }

  .end {
    fill: var(--panel);
    stroke: var(--tint-text);
    stroke-width: 2px;
    vector-effect: non-scaling-stroke;
  }

  .target {
    fill: transparent;
    cursor: crosshair;
  }
</style>
