<script lang="ts">
  import type { Snippet } from 'svelte';
  import { SvelteMap } from 'svelte/reactivity';
  import { text } from './text';

  let {
    children,
    enabled = true,
    room = $bindable(),
    frameKey,
    label = text.viewport.label,
    onmanual,
    onreset
  }: {
    children: Snippet;
    enabled?: boolean;
    room?: DOMRectReadOnly | undefined;
    frameKey: string;
    label?: string;
    onmanual?: () => void;
    onreset?: () => void;
  } = $props();
  const words = text.viewport;
  const help = $props.id();
  const ZOOM = 1.22;
  const START_DISTANCE = 5;
  let viewport: HTMLDivElement | undefined = $state();
  let scale = $state(1);
  let x = $state(0);
  let y = $state(0);
  let moving = $state(false);
  let suppressClick = false;
  let pinchStart: { scale: number; distance: number } | undefined;
  interface Pointer {
    startX: number;
    startY: number;
    x: number;
    y: number;
  }
  const pointers = new SvelteMap<number, Pointer>();

  function reset(): void {
    x = 0;
    y = 0;
    scale = 1;
    pinchStart = undefined;
    onreset?.();
  }

  $effect(() => {
    // A fitted frame is also the new origin for the interactive view.
    if (enabled && frameKey) reset();
  });

  $effect(() => {
    if (!viewport || !enabled) return;
    const node = viewport;
    function wheel(event: WheelEvent): void {
      event.preventDefault();
      zoomTo(event.deltaY < 0 ? scale * ZOOM : scale / ZOOM, event.clientX, event.clientY);
    }
    function click(event: MouseEvent): void {
      if (!suppressClick || event.detail === 0) return;
      suppressClick = false;
      event.preventDefault();
      event.stopPropagation();
    }
    node.addEventListener('wheel', wheel, { passive: false });
    node.addEventListener('click', click, true);
    return () => {
      node.removeEventListener('wheel', wheel);
      node.removeEventListener('click', click, true);
    };
  });

  function zoomTo(targetScale: number, clientX?: number, clientY?: number): void {
    const next = Math.min(10, Math.max(0.3, targetScale));
    if (next === scale) return;
    const box = viewport?.getBoundingClientRect();
    const offsetX = clientX === undefined || !box ? 0 : clientX - box.left - box.width / 2;
    const offsetY = clientY === undefined || !box ? 0 : clientY - box.top - box.height / 2;
    const ratio = next / scale;
    x += (offsetX - x) * (1 - ratio);
    y += (offsetY - y) * (1 - ratio);
    scale = next;
    onmanual?.();
  }

  function pan(dx: number, dy: number): void {
    if (!dx && !dy) return;
    x += dx;
    y += dy;
    onmanual?.();
  }

  function down(event: PointerEvent): void {
    if (
      !enabled ||
      event.button !== 0 ||
      pointers.size >= 2 ||
      (event.target instanceof Element && event.target.closest('button, input, select, a'))
    )
      return;
    if (!pointers.size) suppressClick = false;
    const { clientX: x, clientY: y } = event;
    pointers.set(event.pointerId, { startX: x, startY: y, x, y });
    if (pointers.size === 2) {
      moving = true;
      suppressClick = true;
      for (const id of pointers.keys()) viewport?.setPointerCapture(id);
    }
  }

  function move(event: PointerEvent): void {
    const pointer = pointers.get(event.pointerId);
    if (!pointer || !enabled) return;
    const { clientX, clientY } = event;
    if (!moving) {
      if (Math.hypot(clientX - pointer.startX, clientY - pointer.startY) < START_DISTANCE) return;
      viewport?.setPointerCapture(event.pointerId);
      moving = true;
      suppressClick = true;
    }
    const dx = clientX - pointer.x;
    const dy = clientY - pointer.y;
    const sibling = [...pointers.values()].find((point) => point !== pointer);
    if (sibling) {
      pinchStart ??= {
        scale,
        distance: Math.hypot(pointer.x - sibling.x, pointer.y - sibling.y)
      };
      const distance = Math.hypot(clientX - sibling.x, clientY - sibling.y);
      // Start from the unclamped gesture so returning to the same separation
      // keeps the same zoom, even if one pointer briefly moves beyond a limit.
      if (pinchStart.distance > 0) {
        zoomTo(
          (pinchStart.scale * distance) / pinchStart.distance,
          (pointer.x + sibling.x) / 2,
          (pointer.y + sibling.y) / 2
        );
      } else pinchStart.distance = distance;
      pan(dx / 2, dy / 2);
    } else pan(dx, dy);
    pointer.x = clientX;
    pointer.y = clientY;
  }

  function up(event: PointerEvent): void {
    if (!pointers.delete(event.pointerId)) return;
    pinchStart = undefined;
    if (!pointers.size) moving = false;
  }

  function key(event: KeyboardEvent): void {
    if (!enabled || event.target !== viewport || event.ctrlKey || event.metaKey || event.altKey)
      return;
    const amount = event.shiftKey ? 60 : 20;
    switch (event.key) {
      case 'ArrowLeft':
        pan(-amount, 0);
        break;
      case 'ArrowRight':
        pan(amount, 0);
        break;
      case 'ArrowUp':
        pan(0, -amount);
        break;
      case 'ArrowDown':
        pan(0, amount);
        break;
      case '+':
      case '=':
        zoomTo(scale * ZOOM);
        break;
      case '-':
        zoomTo(scale / ZOOM);
        break;
      case '0':
        reset();
        break;
      default:
        return;
    }
    event.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (The focused canvas supports pan and zoom.) -->
<div
  class="viewport"
  class:interactive={enabled}
  class:moving
  role={enabled ? 'application' : undefined}
  aria-label={enabled ? label : undefined}
  aria-describedby={enabled ? help : undefined}
  tabindex={enabled ? 0 : undefined}
  bind:this={viewport}
  bind:contentRect={room}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onlostpointercapture={(event) => {
    // On touch, taking capture from a child sends a bubbling loss to that child.
    if (event.target === viewport) up(event);
  }}
  onpointerleave={(event) => {
    if (!moving) up(event);
  }}
  onkeydown={key}
>
  <div
    class="scene"
    style:transform={enabled
      ? `translate(${String(x)}px, ${String(y)}px) scale(${String(scale)})`
      : undefined}
  >
    {@render children()}
  </div>
</div>
{#if enabled}
  <div class="controls button-row">
    <button
      type="button"
      class="button"
      aria-label={words.zoomOut}
      onclick={() => {
        zoomTo(scale / ZOOM);
      }}
    >
      −
    </button>
    <button
      type="button"
      class="button"
      aria-label={words.zoomIn}
      onclick={() => {
        zoomTo(scale * ZOOM);
      }}
    >
      +
    </button>
    <button type="button" class="button" onclick={reset}>{words.reset}</button>
    <span id={help} class="note">{words.help}</span>
  </div>
{/if}

<style>
  .viewport {
    flex: 1;
    min-height: 0;
    padding: clamp(8px, 2vw, 22px);
    display: grid;
    place-items: center;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
    overflow: hidden;
  }

  .interactive {
    touch-action: none;
    cursor: grab;
  }
  .moving {
    cursor: grabbing;
    user-select: none;
  }
  .viewport:focus-visible {
    outline-offset: -3px;
  }
  .scene {
    display: grid;
    place-items: center;
    min-width: 0;
  }
  .scene :global(img) {
    -webkit-user-drag: none;
  }

  .controls {
    flex: none;
    gap: 5px 8px;
    padding: 5px 10px;
    border-top: 1px solid var(--line);
    background: var(--panel);
  }

  .controls .button {
    min-height: 32px;
    padding-inline: 8px;
    font-size: 10px;
  }
  @media (max-width: 620px) {
    .controls .note {
      position: absolute;
      clip-path: inset(50%);
      width: 1px;
      height: 1px;
      overflow: hidden;
    }
  }
</style>
