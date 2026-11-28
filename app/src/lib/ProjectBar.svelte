<script lang="ts">
  import { text } from './text';
  import { THEMES, type Theme, type ThemeChoice } from './theme.svelte';
  import type { Workspace } from './workspace.svelte';

  let {
    workspace,
    theme,
    pickTheme,
    projectButton = $bindable<HTMLButtonElement | undefined>(),
    projects,
    actions,
    files,
    back
  }: {
    workspace: Workspace;
    theme: Theme;
    pickTheme: (theme: ThemeChoice) => void;
    projectButton: HTMLButtonElement | undefined;
    projects: () => void;
    actions: () => void;
    files: () => void;
    back: () => void;
  } = $props();
  const words = text.workspace;
  const active = $derived(!!workspace.id && !workspace.starting);
  const inFiles = $derived(workspace.bench.screen === 'files');
</script>

<header class:active class="project-bar">
  <p class="brand">
    <svg width="28" height="28" viewBox="0 0 34 34" aria-hidden="true">
      <path fill="var(--text)" d="M1 1h32v32H1z" />
      <path fill="var(--panel)" d="M6 6h9v9H6zM19 19h9v9h-9z" />
      <path fill="var(--tint-text)" d="M19 6h9v9h-9z" />
    </svg>
    <span>{text.wordmark}.</span>
  </p>

  <div class="identity">
    <button
      type="button"
      class="project"
      bind:this={projectButton}
      aria-haspopup="dialog"
      onclick={projects}
      disabled={workspace.initialising || workspace.busy}
    >
      <span>{active ? workspace.title : words.recent}</span><span aria-hidden="true">⌄</span>
    </button>
    {#if active}
      <button
        type="button"
        class="icon-button secondary"
        aria-label={words.projectActions}
        aria-haspopup="dialog"
        disabled={workspace.busy}
        onclick={actions}
      >
        <span aria-hidden="true">⋯</span>
      </button>
      {#if workspace.sample}
        <button type="button" class="quiet" disabled={workspace.busy} onclick={workspace.keep}>
          {words.keep}
        </button>
      {/if}
      <span class="note state" aria-live="polite">{words[workspace.status]}</span>
    {/if}
  </div>

  <div class="actions">
    {#if active}
      <button type="button" class="button" onclick={workspace.goNew} disabled={workspace.busy}>
        {words.newProject}
      </button>
      {#if inFiles}
        <button type="button" class="button primary" onclick={back}>
          <span aria-hidden="true">←</span>
          {workspace.bench.isFreeform ? text.freeform.back : text.toolbar.back}
        </button>
      {:else}
        <button
          type="button"
          class="button primary"
          onclick={files}
          disabled={!workspace.bench.canShowFiles || workspace.busy}
        >
          {text.toolbar.files} <span aria-hidden="true">→</span>
        </button>
      {/if}
    {/if}
    <select
      class="field-input theme"
      aria-label={text.theme.label}
      value={theme.choice}
      onchange={(event) => {
        pickTheme(event.currentTarget.value as ThemeChoice);
      }}
    >
      {#each THEMES as choice (choice)}
        <option value={choice}>{text.theme[choice]}</option>
      {/each}
    </select>
  </div>
</header>

<style>
  .project-bar,
  .brand,
  .identity,
  .actions {
    display: flex;
    align-items: center;
  }

  .project-bar {
    flex: none;
    min-height: 58px;
    gap: 14px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }

  .brand {
    gap: 8px;
    white-space: nowrap;
    font: 25px/1 var(--serif);
  }

  .identity,
  .actions {
    gap: 8px;
    min-width: 0;
  }

  .identity {
    flex: 1;
  }
  .actions {
    flex: none;
  }

  .project {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 0;
    min-height: 40px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    background: var(--panel);
    font-weight: 620;
  }

  .project span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .secondary {
    flex: none;
    font-size: 22px;
    border: 1px solid var(--line);
  }

  .state {
    line-height: 1.2;
  }
  .theme {
    width: auto;
  }

  @media (max-width: 760px) {
    .project-bar {
      flex-wrap: wrap;
      gap: 6px 8px;
      padding: 6px 10px;
    }
    .brand {
      font-size: 22px;
      gap: 6px;
    }
    .brand svg {
      width: 23px;
      height: 23px;
    }
    .active .actions {
      flex-basis: 100%;
      justify-content: flex-end;
      flex-wrap: wrap;
    }
    .active .identity {
      flex: 1 1 0;
    }
    .state {
      font-size: 11px;
    }
    .actions .button {
      padding-inline: 8px;
      min-height: 36px;
      font-size: 10px;
    }
    .theme {
      min-height: 36px;
      padding-inline: 3px;
      font-size: 11px;
    }
  }

  @media (max-width: 370px) {
    .active .identity {
      flex-basis: 100%;
    }
    .active .actions {
      justify-content: flex-start;
    }
  }
</style>
