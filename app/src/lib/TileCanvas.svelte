<script lang="ts">
  // Draws a chart from its tile labels: with grid lines, without them, or as one
  // colour's stencil, a plate with a hole over each of that colour's tiles.
  import type { ChartSummary } from '../engine/client';

  interface Props {
    chart: ChartSummary;
    /** `#rrggbb` per palette entry. */
    colours: readonly string[];
    /** Width on screen, in CSS pixels. */
    width: number;
    label: string;
    gridColour?: string;
    withLines?: boolean;
    /** Draw this palette entry's stencil instead of the chart. */
    stencil?: number | undefined;
    /** Whether the page is dark, which decides the colour of a stencil's plate. */
    dark?: boolean;
  }

  let {
    chart,
    colours,
    width,
    label,
    gridColour = '#000000',
    withLines = true,
    stencil,
    dark = false
  }: Props = $props();

  /** Plastic on paper: dark on a light page, light on a dark one. */
  const PLATE = { light: '#404040', dark: '#7a7a7a' };

  let canvas: HTMLCanvasElement | undefined = $state();

  const line = $derived(withLines || stencil !== undefined ? chart.linePx : 0);
  const pitch = $derived(chart.tilePx + line);
  const fullWidth = $derived(chart.tilesX * pitch + line);
  const fullHeight = $derived(chart.tilesY * pitch + line);
  const height = $derived(fullWidth > 0 ? (width * fullHeight) / fullWidth : 0);

  $effect(() => {
    const context = canvas?.getContext('2d');
    if (!canvas || !context || width <= 0 || fullWidth <= 0) return;
    const scale = (width * window.devicePixelRatio) / fullWidth;
    canvas.width = Math.max(1, Math.round(fullWidth * scale));
    canvas.height = Math.max(1, Math.round(fullHeight * scale));
    // Tile edges on whole device pixels, so lines stay crisp.
    const starts = (count: number) =>
      Array.from({ length: count }, (_, i) => Math.round((line + i * pitch) * scale));
    const ends = (count: number) =>
      Array.from({ length: count }, (_, i) =>
        Math.round((line + i * pitch + chart.tilePx) * scale)
      );
    const left = starts(chart.tilesX);
    const right = ends(chart.tilesX);
    const top = starts(chart.tilesY);
    const bottom = ends(chart.tilesY);
    const tile = (path: Path2D, x: number, y: number) => {
      const x0 = left[x] ?? 0;
      const y0 = top[y] ?? 0;
      path.rect(x0, y0, (right[x] ?? x0) - x0, (bottom[y] ?? y0) - y0);
    };

    if (stencil === undefined) {
      context.fillStyle = gridColour;
      context.fillRect(0, 0, canvas.width, canvas.height);
      const paths = colours.map(() => new Path2D());
      for (let y = 0; y < chart.tilesY; y++) {
        for (let x = 0; x < chart.tilesX; x++) {
          const path = paths[chart.labels[y * chart.tilesX + x] ?? 0];
          if (path) tile(path, x, y);
        }
      }
      paths.forEach((path, i) => {
        context.fillStyle = colours[i] ?? gridColour;
        context.fill(path);
      });
    } else {
      context.globalCompositeOperation = 'source-over';
      context.fillStyle = dark ? PLATE.dark : PLATE.light;
      context.fillRect(0, 0, canvas.width, canvas.height);
      const holes = new Path2D();
      for (let y = 0; y < chart.tilesY; y++) {
        for (let x = 0; x < chart.tilesX; x++) {
          if (chart.labels[y * chart.tilesX + x] === stencil) tile(holes, x, y);
        }
      }
      context.globalCompositeOperation = 'destination-out';
      context.fill(holes);
      context.globalCompositeOperation = 'source-over';
    }
  });
</script>

<div class="frame" role="img" aria-label={label} style:width="{width}px" style:height="{height}px">
  <canvas bind:this={canvas} aria-hidden="true"></canvas>
</div>

<style>
  .frame {
    flex: none;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
