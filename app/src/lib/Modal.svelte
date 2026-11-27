<script lang="ts">
  import type { Snippet } from 'svelte';
  import { text } from './text';

  let {
    title,
    dismiss,
    busy = false,
    children
  }: { title: string; dismiss: () => void; busy?: boolean; children: Snippet } = $props();
  let dialog: HTMLDialogElement | undefined = $state();
  let heading: HTMLHeadingElement | undefined = $state();
  const id = $props.id();

  $effect(() => {
    const node = dialog;
    if (!node) return;
    node.showModal();
    let startedOutside = false;
    const outside = (event: PointerEvent): boolean => {
      const { left, right, top, bottom } = node.getBoundingClientRect();
      return (
        event.clientX < left ||
        event.clientX > right ||
        event.clientY < top ||
        event.clientY > bottom
      );
    };
    const down = (event: PointerEvent): void => {
      startedOutside = outside(event);
    };
    const up = (event: PointerEvent): void => {
      if (startedOutside && outside(event) && !busy) dismiss();
    };
    node.addEventListener('pointerdown', down);
    node.addEventListener('pointerup', up);
    return () => {
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointerup', up);
      node.close();
    };
  });

  $effect(() => {
    // The same dialog can move from setup to a restore preview or profile editor.
    if (title && dialog?.open) heading?.focus();
  });
</script>

<dialog
  class="quiet-scroll"
  bind:this={dialog}
  aria-labelledby={id}
  aria-busy={busy}
  oncancel={(event) => {
    event.preventDefault();
    if (!busy) dismiss();
  }}
>
  <header>
    <h2 {id} tabindex="-1" bind:this={heading}>{title}</h2>
    <button
      type="button"
      class="icon-button"
      aria-label={text.workspace.close}
      disabled={busy}
      onclick={dismiss}
    >
      <span aria-hidden="true">×</span>
    </button>
  </header>
  <div class="body">
    {@render children()}
  </div>
</dialog>

<style>
  dialog {
    width: min(540px, calc(100% - 24px));
    max-height: calc(100dvh - 24px);
    padding: 0;
    border: 1px solid var(--line-strong);
    background: var(--panel);
    color: var(--text);
  }

  dialog::backdrop {
    background: rgb(0 0 0 / 48%);
  }

  header {
    padding: 10px 16px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    border-bottom: 1px solid var(--line);
  }

  h2 {
    font: 400 25px/1.2 var(--serif);
  }

  .icon-button {
    font-size: 25px;
  }

  .body {
    padding: 16px;
    display: grid;
    gap: 12px;
    overflow-wrap: anywhere;
  }
</style>
