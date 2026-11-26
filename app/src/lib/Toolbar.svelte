<script lang="ts">
  // Above the bench: undo and redo, what the bench shows, and the way to the files.
  import Segmented from './Segmented.svelte';
  import { text } from './text';
  import type { View, Workbench } from './workbench.svelte';

  interface Props {
    bench: Workbench;
  }

  let { bench }: Props = $props();

  const views = $derived<{ value: View; label: string; disabled: boolean }[]>([
    { value: 'picture', label: text.toolbar.views.picture, disabled: !bench.source },
    { value: 'chart', label: text.toolbar.views.chart, disabled: !bench.chart },
    { value: 'both', label: text.toolbar.views.both, disabled: !bench.chart },
    { value: 'stencil', label: text.toolbar.views.stencil, disabled: !bench.stencilsPossible }
  ]);
  const onFiles = $derived(bench.screen === 'files');
</script>

<div class="toolbar">
  <div class="start">
    <button
      type="button"
      class="icon-button"
      aria-label={text.toolbar.undo}
      title={text.toolbar.undo}
      disabled={!bench.canUndo}
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
      disabled={!bench.canRedo}
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

  <div class="middle">
    {#if !onFiles}
      <Segmented
        label={text.toolbar.view}
        options={views}
        value={bench.view}
        onpick={(view: View) => {
          bench.show(view);
        }}
      />
    {/if}
  </div>

  <div class="end">
    {#if onFiles}
      <button
        type="button"
        class="button"
        onclick={() => {
          bench.go('bench');
        }}
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          aria-hidden="true"
        >
          <path d="M10 3L5 8l5 5" />
        </svg>
        {text.toolbar.back}
      </button>
    {:else}
      <button
        type="button"
        class="button primary"
        disabled={!bench.chart}
        onclick={() => {
          bench.go('files');
        }}
      >
        {text.toolbar.files}
        <svg
          width="16"
          height="16"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          aria-hidden="true"
        >
          <path d="M6 3l5 5-5 5" />
        </svg>
      </button>
    {/if}
  </div>
</div>

<style>
  .toolbar {
    min-height: 56px;
    padding: 8px 12px;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 12px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }

  .start {
    display: flex;
    gap: 4px;
  }

  .end {
    display: flex;
    justify-content: flex-end;
  }

  @media (max-width: 720px) {
    .toolbar {
      grid-template-columns: auto 1fr;
    }

    .middle {
      grid-row: 2;
      grid-column: 1 / -1;
      overflow-x: auto;
    }
  }
</style>
