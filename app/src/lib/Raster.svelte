<script lang="ts">
  // A bounded RGBA preview from the engine, drawn without reprocessing its pixels.
  import type { RasterPreview } from '../engine/client';

  interface Props {
    preview: RasterPreview;
    label: string;
  }

  let { preview, label }: Props = $props();
  let canvas: HTMLCanvasElement | undefined = $state();

  $effect(() => {
    if (!canvas) return;
    const { width, height, rgba } = preview;
    canvas.width = width;
    canvas.height = height;
    canvas
      .getContext('2d')
      ?.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
  });
</script>

<div class="raster" role="img" aria-label={label}>
  <canvas bind:this={canvas} aria-hidden="true"></canvas>
</div>

<style>
  .raster,
  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
