<script lang="ts">
  // A millimetre ruler along one edge of a sheet, for orientation rather than measuring:
  // a screen does not show millimetres at their real size.
  import { positions, tickSteps } from './ruler';
  import { num } from './text';

  interface Props {
    lengthMm: number;
    sizePx: number;
    vertical?: boolean;
  }

  let { lengthMm, sizePx, vertical = false }: Props = $props();

  const DEPTH = 22;
  const scale = $derived(lengthMm > 0 ? sizePx / lengthMm : 0);
  const steps = $derived(tickSteps(scale));
  const minor = $derived(positions(lengthMm, steps.minor));
  // The unit sits at the far end, so labels too close to it give way.
  const major = $derived(
    positions(lengthMm, steps.major).filter((mm) => mm * scale < sizePx - (vertical ? 50 : 40))
  );
  const at = (mm: number): number => Math.round(mm * scale) + 0.5;
</script>

<svg
  class="ruler"
  width={vertical ? DEPTH : sizePx}
  height={vertical ? sizePx : DEPTH}
  aria-hidden="true"
>
  {#if vertical}
    {#each minor as mm (mm)}
      <line x1={DEPTH - 4} x2={DEPTH} y1={at(mm)} y2={at(mm)} />
    {/each}
    {#each major as mm (mm)}
      <line x1={DEPTH - 8} x2={DEPTH} y1={at(mm)} y2={at(mm)} />
      <text x={10} y={at(mm) + 3} text-anchor="end" transform="rotate(-90 10 {at(mm) + 3})">
        {num(mm, 1)}
      </text>
    {/each}
    <text x={10} y={sizePx - 2} transform="rotate(-90 10 {sizePx - 2})">mm</text>
  {:else}
    {#each minor as mm (mm)}
      <line x1={at(mm)} x2={at(mm)} y1={DEPTH - 4} y2={DEPTH} />
    {/each}
    {#each major as mm (mm)}
      <line x1={at(mm)} x2={at(mm)} y1={DEPTH - 8} y2={DEPTH} />
      <text x={at(mm) + 3} y={10}>{num(mm, 1)}</text>
    {/each}
    <text x={sizePx - 3} y={10} text-anchor="end">mm</text>
  {/if}
</svg>

<style>
  .ruler {
    display: block;
    background: var(--ruler);
    outline: 1px solid var(--line);
    outline-offset: -1px;
  }

  line {
    stroke: var(--tick);
    stroke-width: 1;
  }

  text {
    fill: var(--text-3);
    font-size: 9px;
    font-variant-numeric: tabular-nums;
  }
</style>
