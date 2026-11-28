<script lang="ts" generics="T extends string">
  // A row of buttons of which exactly one is pressed.
  interface Option {
    value: T;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    label: string;
    options: readonly Option[];
    value: T;
    onpick: (value: T) => void;
    /** Share the width equally, as in the settings panel. */
    stretch?: boolean;
    wrap?: boolean;
  }

  let { label, options, value, onpick, stretch = false, wrap = false }: Props = $props();
</script>

<div class="segmented" class:stretch class:wrap role="group" aria-label={label}>
  {#each options as option (option.value)}
    <button
      type="button"
      aria-pressed={option.value === value}
      disabled={option.disabled ?? false}
      onclick={() => {
        onpick(option.value);
      }}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  .segmented {
    display: inline-flex;
    border: 1px solid var(--line-strong);
  }

  .stretch {
    display: flex;
  }

  .wrap {
    flex-wrap: wrap;
    gap: 3px;
    border: 0;
  }

  button {
    min-height: 32px;
    padding: 0 12px;
    border: 0;
    background: transparent;
    color: var(--text-2);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .stretch button {
    flex: 1;
    min-height: 36px;
    padding: 0 6px;
  }

  button + button {
    border-left: 1px solid var(--line-strong);
  }

  .wrap button {
    padding-inline: 8px;
    border: 1px solid var(--line);
    font-size: 10px;
  }

  button:hover:not(:disabled) {
    color: var(--text);
  }

  button[aria-pressed='true'] {
    background: var(--tint);
    color: var(--tint-text);
    box-shadow: inset 0 0 0 1px var(--tint-line);
  }

  button:disabled {
    color: var(--text-3);
    opacity: 0.5;
  }
</style>
