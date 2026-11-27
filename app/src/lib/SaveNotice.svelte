<script lang="ts">
  import { text } from './text';
  import type { Workspace } from './workspace.svelte';

  let { workspace }: { workspace: Workspace } = $props();
  const words = text.workspace;
  const status = $derived(workspace.status);
  const missing = $derived(!workspace.remote);
</script>

{#if status === 'failed' || status === 'conflict' || status === 'upgrade'}
  <section class="recovery notice" aria-label={words[status]}>
    <div>
      <p><strong>{status === 'conflict' && missing ? words.removed : words[status]}</strong></p>
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
          disabled={workspace.busy}
          onclick={() => void workspace.flush()}
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
