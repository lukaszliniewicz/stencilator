<script lang="ts">
  import type { ChartSummary } from '../engine/client';
  import { rgbToHex } from './colour';

  interface Props {
    chart: ChartSummary;
    gridColour: string;
    withLines: boolean;
  }

  let { chart, gridColour, withLines }: Props = $props();
  let canvas: HTMLCanvasElement | undefined = $state();
  let width = $state(0);

  $effect(() => {
    const context = canvas?.getContext('2d');
    if (!canvas || !context || width === 0) return;
    const line = withLines ? chart.linePx : 0;
    const pitch = chart.tilePx + line;
    const fullWidth = chart.tilesX * pitch + line;
    const fullHeight = chart.tilesY * pitch + line;
    const scale = (width / fullWidth) * window.devicePixelRatio;
    canvas.width = Math.round(fullWidth * scale);
    canvas.height = Math.round(fullHeight * scale);
    context.fillStyle = gridColour;
    context.fillRect(0, 0, canvas.width, canvas.height);
    const colours = Array.from({ length: chart.palette.length / 3 }, (_, i) =>
      rgbToHex(
        chart.palette[3 * i] ?? 0,
        chart.palette[3 * i + 1] ?? 0,
        chart.palette[3 * i + 2] ?? 0
      )
    );
    const edge = (index: number) => Math.round((line + index * pitch) * scale);
    for (let y = 0; y < chart.tilesY; y++) {
      for (let x = 0; x < chart.tilesX; x++) {
        context.fillStyle = colours[chart.labels[y * chart.tilesX + x] ?? 0] ?? gridColour;
        const left = edge(x);
        const top = edge(y);
        context.fillRect(
          left,
          top,
          Math.round((line + x * pitch + chart.tilePx) * scale) - left,
          Math.round((line + y * pitch + chart.tilePx) * scale) - top
        );
      }
    }
  });
</script>

<div
  class="frame"
  role="img"
  aria-label="The chart, {chart.tilesX} by {chart.tilesY} tiles"
  bind:clientWidth={width}
>
  <canvas bind:this={canvas} aria-hidden="true"></canvas>
</div>

<style>
  .frame {
    width: 100%;
  }

  canvas {
    display: block;
    width: 100%;
    height: auto;
  }
</style>
