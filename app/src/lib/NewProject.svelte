<script lang="ts">
  import { onMount } from 'svelte';
  import { text } from './text';
  import type { Preset } from './workbench.svelte';
  import type { Workspace } from './workspace.svelte';

  let {
    workspace,
    preset,
    choosePreset,
    pick,
    importProject,
    resume
  }: {
    workspace: Workspace;
    preset: Preset;
    choosePreset: (preset: Preset) => void;
    pick: () => void;
    importProject: () => void;
    resume: () => void;
  } = $props();
  const words = text.workspace;
  const id = $props.id();
  const presets = ['pixel', 'tile', 'freeform'] as const;
  const busy = $derived(workspace.initialising || workspace.busy);
  let heading: HTMLHeadingElement | undefined = $state();

  onMount(() => {
    if (workspace.id) heading?.focus();
  });
</script>

<main class="start quiet-scroll">
  <section aria-labelledby={`${id}-title`}>
    <header>
      <p class="label">{words.newProject}</p>
      <h1 id={`${id}-title`} bind:this={heading} tabindex="-1">{words.chooserTitle}</h1>
      <p class="intro">{words.chooserBody}</p>
    </header>
    <div class="presets" role="group" aria-label={words.presetLabel}>
      {#each presets as key (key)}
        <button
          type="button"
          aria-label={words.presets[key].title}
          aria-describedby={`${id}-${key}`}
          aria-pressed={preset === key}
          disabled={busy}
          onclick={() => {
            choosePreset(key);
          }}
        >
          <svg viewBox="0 0 64 36" aria-hidden="true">
            {#if key === 'pixel'}
              <path
                fill="currentColor"
                d="M15 4h8v8h17V4h8v8h5v11h-8v5h-8v5H27v-5h-8v-5h-8V12h4z"
              />
              <path fill="var(--panel)" d="M18 18h7v6h-7zm21 0h7v6h-7zM28 26h8v4h-8z" />
            {:else if key === 'tile'}
              <path
                d="M12 5h40v26H12zM22 5v26M32 5v26M42 5v26M12 14h40M12 22h40"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
              />
              <path
                fill="currentColor"
                d="M32 5h10v9H32zm10 9h10v8H42zM22 22h10v9H22zm10-8h10v8H32z"
              />
            {:else}
              <path
                fill="currentColor"
                fill-rule="evenodd"
                d="M12 4h40v28H12zm8 22c6-1 8-8 12-12 4-4 9-4 14-4-1 8-4 17-13 17-4 0-6-2-7-3-2 2-4 3-6 3z"
              />
            {/if}
          </svg>
          <strong>{words.presets[key].title}</strong>
          <span id={`${id}-${key}`}>{words.presets[key].body}</span>
        </button>
      {/each}
    </div>
    <div class="start-options">
      <div class="button-row">
        <button type="button" class="button primary" disabled={busy} onclick={pick}>
          {words.choose}
        </button>
        <button
          type="button"
          class="button"
          disabled={busy}
          onclick={() => void workspace.startSample(preset)}
        >
          {preset === 'freeform' ? text.freeform.example : words.tryFox}
        </button>
        {#if preset === 'freeform'}
          <button
            type="button"
            class="button"
            disabled={busy}
            onclick={() => void workspace.startText()}
          >
            {text.freeform.type}
          </button>
        {/if}
      </div>
      <p class="note">{preset === 'freeform' ? words.drawTypes : words.imageTypes}</p>
    </div>
    <footer class="button-row">
      <button type="button" class="quiet" disabled={busy} onclick={importProject}>
        {text.project.open}
      </button>
      <a
        class="quiet"
        href="https://github.com/lukaszliniewicz/stencilator#make-your-first-stencil"
      >
        {words.guide}
      </a>
      {#if workspace.id}
        <button type="button" class="quiet" disabled={busy} onclick={resume}>{words.back}</button>
      {/if}
      <p class="note">{text.privacy}</p>
      {#if workspace.bench.version}<p class="note">{text.engine(workspace.bench.version)}</p>{/if}
    </footer>
    {#if workspace.initialising}<p class="note" role="status">{words.startup}</p>{/if}
  </section>
</main>

<style>
  .start {
    flex: 1;
    display: grid;
    place-items: safe center;
    min-height: 0;
    overflow: auto;
    padding: 20px 14px;
    background: var(--surface) var(--canvas-grid);
    background-size: 20px 20px;
  }

  section {
    width: min(100%, 780px);
    display: grid;
    gap: clamp(14px, 3vw, 24px);
    padding: clamp(16px, 4vw, 38px);
    border: 1px solid var(--line-strong);
    background: var(--panel);
  }

  h1 {
    margin-top: 4px;
    font: 400 clamp(30px, 4vw, 46px)/1.15 var(--serif);
  }

  .intro {
    max-width: 49ch;
    margin-top: 10px;
    color: var(--text-2);
  }

  .presets {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }

  .presets button {
    display: grid;
    align-content: start;
    justify-items: start;
    gap: 6px;
    min-width: 0;
    padding: 12px;
    text-align: left;
    border: 1px solid var(--line-strong);
    background: var(--panel);
  }

  .presets button[aria-pressed='true'] {
    border-color: var(--focus);
    background: var(--tint);
  }

  .presets svg {
    width: 56px;
    height: 37px;
    color: var(--tint-text);
  }
  .presets strong {
    font: 400 20px/1.2 var(--serif);
  }
  .presets span {
    color: var(--text-2);
    font-size: 12px;
  }
  .start-options p {
    margin-top: 8px;
  }
  footer {
    border-top: 1px solid var(--line);
    padding-top: 8px;
  }

  @media (max-width: 620px) {
    .presets {
      grid-template-columns: 1fr;
      gap: 6px;
    }
    .presets button {
      grid-template-columns: 42px 1fr;
      gap: 2px 8px;
      padding: 8px;
    }
    .presets svg {
      grid-row: span 2;
      width: 40px;
      align-self: center;
    }
    .presets strong {
      font-size: 18px;
    }
  }
</style>
