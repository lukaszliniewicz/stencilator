<script lang="ts" module>
  /** Room the rulers and the caption take around a sheet, in CSS pixels. */
  export const RULER_SPACE = 26;
  export const CAPTION_SPACE = 28;
</script>

<script lang="ts">
  // A sheet on the bench: its caption, rulers along the top and left edge, and whatever
  // is drawn on it, at a width the bench has worked out.
  import type { Snippet } from 'svelte';

  import Ruler from './Ruler.svelte';

  interface Props {
    widthMm: number;
    heightMm: number;
    /** Width of the sheet itself, in CSS pixels. */
    width: number;
    start: string;
    end: string;
    faded?: boolean;
    children: Snippet<[{ width: number; height: number }]>;
  }

  let { widthMm, heightMm, width, start, end, faded = false, children }: Props = $props();
  const height = $derived(widthMm > 0 ? (width * heightMm) / widthMm : 0);
</script>

<figure class="sheet" class:faded>
  <figcaption class="label">
    <span>{start}</span>
    <span>{end}</span>
  </figcaption>
  <div class="rulers">
    <span class="corner" aria-hidden="true"></span>
    <Ruler lengthMm={widthMm} sizePx={width} />
    <Ruler lengthMm={heightMm} sizePx={height} vertical />
    <div class="content" style:width="{width}px" style:height="{height}px">
      {@render children({ width, height })}
    </div>
  </div>
</figure>

<style>
  .sheet {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition: opacity 0.2s 0.15s;
  }

  .faded {
    opacity: 0.4;
  }

  figcaption {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding-left: 26px;
    line-height: 18px;
  }

  .rulers {
    display: grid;
    grid-template-columns: 22px auto;
    gap: 4px;
  }

  .corner {
    width: 22px;
    height: 22px;
  }

  .content {
    position: relative;
    display: flex;
  }
</style>
