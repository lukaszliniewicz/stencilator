<script lang="ts">
  import type { Coat, FreeformSummary } from '../engine/client';
  import { text } from './text';

  interface Props {
    summary: FreeformSummary;
    coats: Coat[] | undefined;
    width: number;
    faded: boolean;
  }

  let { summary, coats, width, faded }: Props = $props();
  let step = $state(-1);
  const currentStep = $derived(step < (coats?.length ?? 0) ? step : -1);
  const visible = $derived(currentStep < 0 ? coats : coats?.slice(0, currentStep));
</script>

<figure class="result" class:faded style:width="{width}px">
  <figcaption class="label">{text.freeform.views.result}</figcaption>
  {#if coats}
    {#if coats.length > 1}
      <label class="coats">
        <span class="label">{text.coating.options}</span>
        <select
          value={currentStep}
          onchange={(event) => (step = Number(event.currentTarget.value))}
        >
          {#each coats.slice(0, -1) as coat, i (coat.paint)}
            <option value={i + 1}>{text.coating.number(i + 1)}</option>
          {/each}
          <option value={-1}>{text.coating.all}</option>
        </select>
      </label>
    {/if}
    <svg
      class="painting"
      viewBox="0 0 {summary.widthMm} {summary.heightMm}"
      role="img"
      aria-label={text.coating.preview}
    >
      <rect width={summary.widthMm} height={summary.heightMm} fill={summary.background} />
      {#each visible ?? [] as coat (coat.paint)}
        <path d={coat.path} fill={coat.colour} fill-rule="evenodd" data-paint={coat.paint} />
      {/each}
    </svg>
    {#if !coats.length}<p class="note">{text.freeform.noneTicked}</p>{/if}
    <p class="note">{text.coating.note}</p>
  {:else}
    <p class="note">{text.coating.loading}</p>
  {/if}
</figure>

<style>
  .result {
    margin: 0;
    max-width: 100%;
    display: grid;
    align-content: start;
    gap: 8px;
    transition: opacity 0.2s 0.15s;
  }

  .faded {
    opacity: 0.4;
  }

  .coats {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }

  select {
    min-height: 32px;
    padding-inline: 6px;
    border: 1px solid var(--line-strong);
    border-radius: 0;
    background: var(--field);
    font-size: 12px;
  }

  .painting {
    width: 100%;
    display: block;
    border: 1px solid var(--line);
    overflow: hidden;
  }
</style>
