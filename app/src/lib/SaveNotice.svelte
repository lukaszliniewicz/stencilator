<script lang="ts">
  import { tick } from 'svelte';
  import { text } from './text';
  import type { Workspace } from './workspace.svelte';

  let { workspace, onRecovered }: { workspace: Workspace; onRecovered: () => void } = $props();
  const words = text.workspace;
  let retrying = $state(false);
  let retryButton: HTMLButtonElement | undefined = $state();
  const status = $derived(retrying && workspace.status === 'saving' ? 'failed' : workspace.status);
  const missing = $derived(!workspace.remote);
  const title = $derived(status === 'conflict' && missing ? words.removed : words[status]);

  async function retry(): Promise<void> {
    if (retrying || workspace.busy) return;
    retrying = true;
    const button = retryButton;
    const wasFocused = document.activeElement === button;
    try {
      await workspace.flush();
    } finally {
      retrying = false;
      await tick();
      if (
        wasFocused &&
        (document.activeElement === button || document.activeElement === document.body)
      ) {
        if (retryButton) retryButton.focus();
        else onRecovered();
      }
    }
  }
</script>

{#if status === 'failed' || status === 'conflict' || status === 'upgrade'}
  <section class="recovery notice" aria-label={title}>
    <div>
      <p><strong>{title}</strong></p>
      <p>
        {status === 'failed'
          ? words.failedHelp
          : status === 'upgrade'
            ? words.upgradeHelp
            : missing
              ? words.removedHelp
              : words.conflictHelp}
      </p>
      {#if status === 'failed' && workspace.saveProblem}
        <p>{workspace.saveProblem}</p>
      {/if}
    </div>
    <div class="button-row">
      {#if status === 'failed'}
        <button
          type="button"
          class="button"
          bind:this={retryButton}
          aria-disabled={workspace.busy || retrying}
          onclick={() => void retry()}
        >
          {words.retry}
        </button>
      {:else if status === 'conflict'}
        {#if !missing}
          <button
            type="button"
            class="button"
            disabled={workspace.busy}
            onclick={() => void workspace.loadRemote()}
          >
            {words.loadNewer}
          </button>
        {/if}
        <button
          type="button"
          class="button primary"
          disabled={workspace.busy}
          onclick={() => void workspace.copy(true)}
        >
          {words.keepCopy}
        </button>
      {:else}
        <button
          type="button"
          class="button"
          onclick={() => {
            window.location.reload();
          }}
        >
          {words.reload}
        </button>
      {/if}
      {#if workspace.id}
        <button
          type="button"
          class="quiet"
          disabled={workspace.busy || !workspace.bench.canSaveProject}
          onclick={() => void workspace.download()}
        >
          {text.project.save}
        </button>
      {/if}
    </div>
  </section>
{/if}

<style>
  .recovery {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 16px;
    border-inline: 0;
    padding-inline: 12px;
  }

  strong {
    font-size: 13px;
  }
</style>
