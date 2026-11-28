<script lang="ts">
  // Above the bench: undo, redo and what the bench shows.
  import Segmented from './Segmented.svelte';
  import { text } from './text';
  import type { View, Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
  }

  let { bench }: Props = $props();

  const views = $derived.by((): { value: View; label: string; disabled: boolean }[] => {
    const labels = bench.isFreeform ? text.freeform.views : text.toolbar.views;
    const views: { value: View; label: string; disabled: boolean }[] = [
      { value: 'picture', label: labels.picture, disabled: !bench.artwork },
      { value: 'chart', label: labels.chart, disabled: !bench.canShowFiles },
      { value: 'both', label: labels.both, disabled: !bench.canShowFiles },
      { value: 'stencil', label: labels.stencil, disabled: !bench.stencilsPossible }
    ];
    if (bench.isFreeform) {
      views.push({
        value: 'result',
        label: text.freeform.views.result,
        disabled: !bench.canShowFiles
      });
    }
    return [...views, { value: 'bed', label: text.bed.title, disabled: !bench.stencilsPossible }];
  });
</script>

<div class="toolbar">
  <div class="start">
    <button
      type="button"
      class="icon-button"
      aria-label={text.toolbar.undo}
      title={text.toolbar.undo}
      disabled={!bench.canUndo || bench.opening}
      onclick={bench.undo}
    >
      <svg
        width="18"
        height="18"
        viewBox="0 0 18 18"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        aria-hidden="true"
      >
        <path d="M6 4L2.5 7.5 6 11" />
        <path d="M3 7.5h7.5a4.5 4.5 0 010 9H8" />
      </svg>
    </button>
    <button
      type="button"
      class="icon-button"
      aria-label={text.toolbar.redo}
      title={text.toolbar.redo}
      disabled={!bench.canRedo || bench.opening}
      onclick={bench.redo}
    >
      <svg
        width="18"
        height="18"
        viewBox="0 0 18 18"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        aria-hidden="true"
      >
        <path d="M12 4l3.5 3.5L12 11" />
        <path d="M15 7.5H7.5a4.5 4.5 0 000 9H10" />
      </svg>
    </button>
  </div>

  <Segmented
    label={text.toolbar.view}
    options={views}
    value={bench.view}
    onpick={bench.show}
    wrap
  />
</div>

<style>
  .toolbar {
    flex: none;
    padding: 5px 10px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }

  .start {
    display: flex;
    gap: 4px;
  }
</style>
