<script lang="ts">
  import type { PlateReport } from '../engine/client';
  import { text } from './text';

  let { plate }: { plate: PlateReport } = $props();
  const words = text.freeform;
  const issues = $derived([
    ...(plate.islands.length ? [words.islands(plate.islands.length)] : []),
    ...(plate.thin_webs.length ? [words.thinWebs(plate.thin_webs.length)] : []),
    ...(plate.narrow_openings.length ? [words.narrowOpenings(plate.narrow_openings.length)] : []),
    ...(!plate.fits_bed ? [words.bedWarning] : [])
  ]);
</script>

{#if issues.length}
  <ul class="issues">
    {#each issues as issue (issue)}
      <li>{issue}</li>
    {/each}
  </ul>
{:else}
  <p class="note">{words.noWarnings}</p>
{/if}

<style>
  .issues {
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    list-style: none;
  }

  .issues li {
    padding: 2px 6px;
    border: 1px solid var(--warn-line);
    background: var(--warn);
    color: var(--warn-text);
    font-size: 11px;
  }
</style>
