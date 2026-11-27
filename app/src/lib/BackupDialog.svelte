<script lang="ts">
  import { save } from './download';
  import Modal from './Modal.svelte';
  import { text } from './text';
  import { workspaceProblem, type Workspace } from './workspace.svelte';
  import type { WorkspaceBackup } from './workspace-backup';
  import type { RestorePlan } from './workspace-store';

  let { workspace, dismiss }: { workspace: Workspace; dismiss: () => void } = $props();
  const words = text.backup;
  let input: HTMLInputElement | undefined = $state();
  let busy = $state(false);
  let checking = $state(false);
  let problem = $state('');
  let note = $state('');
  let excluded = $state('');
  let backup = $state.raw<WorkspaceBackup>();
  let preview = $state.raw<RestorePlan>();
  let preferences = $state(false);

  async function action(task: () => Promise<void>): Promise<void> {
    busy = true;
    note = '';
    problem = '';
    try {
      await task();
    } catch (error) {
      problem = workspaceProblem(error);
    } finally {
      checking = false;
      busy = false;
    }
  }

  function download(savedOnly = false): void {
    void action(async () => {
      const saved = await workspace.flush();
      if (!saved && !savedOnly) {
        excluded = workspace.title;
        return;
      }
      save(await workspace.backup(), 'stencilator-workspace.json', 'application/json');
      excluded = '';
      note = words.ready;
    });
  }

  function inspect(file: File): void {
    checking = true;
    void action(async () => {
      await workspace.flush();
      const decoded = await workspace.readBackup(file);
      preview = await workspace.previewBackup(decoded);
      backup = decoded;
      preferences = false;
    });
  }

  function restore(): void {
    const plan = preview;
    const data = backup;
    if (!plan || !data) return;
    void action(async () => {
      if (await workspace.restoreBackup(plan, preferences)) {
        preview = undefined;
        backup = undefined;
        note = words.imported;
      } else {
        preview = await workspace.previewBackup(data);
        note = words.updated;
      }
    });
  }
</script>

<Modal title={preview ? words.preview : words.title} {dismiss} {busy}>
  <input
    type="file"
    bind:this={input}
    hidden
    accept=".json,application/json"
    aria-label={words.input}
    onchange={(event) => {
      const file = event.currentTarget.files?.[0];
      event.currentTarget.value = '';
      if (file) inspect(file);
    }}
  />

  {#if preview}
    <p class="note">{words.importBody}</p>
    {#if preview.projects.length}
      <h3 class="label">{words.previewProjects}</h3>
      <ul class="quiet-scroll">
        {#each preview.projects as row (row.source.id)}
          <li>
            <div>
              <strong>{row.record.title}</strong>
              <p class="note">
                {text.workspace.edited(text.project.modes[row.source.mode], row.source.editedAt)}
              </p>
            </div>
            <span>{words.actions[row.action]}</span>
          </li>
        {/each}
      </ul>
    {/if}
    {#if preview.profiles.length}
      <h3 class="label">{words.previewProfiles}</h3>
      <ul class="quiet-scroll">
        {#each preview.profiles as row (row.source.id)}
          <li>
            <div>
              <strong>{row.record.name}</strong>
              <p class="note">{text.bed.size(row.source.widthMm, row.source.depthMm)}</p>
            </div>
            <span>{words.actions[row.action]}</span>
          </li>
        {/each}
      </ul>
    {/if}
    {#if !preview.projects.length && !preview.profiles.length}<p class="note">{words.blank}</p>{/if}
    <label class="checkbox">
      <input type="checkbox" bind:checked={preferences} disabled={busy} />
      {words.preferences}
    </label>
    <p class="note">{words.preferencesHint}</p>
    <div class="button-row">
      <button
        type="button"
        class="quiet"
        disabled={busy}
        onclick={() => {
          preview = undefined;
          backup = undefined;
          problem = '';
          note = '';
        }}
      >
        {words.back}
      </button>
      <button type="button" class="button primary" disabled={busy} onclick={restore}>
        {words.import}
      </button>
    </div>
  {:else}
    <section>
      <h3>{words.exportTitle}</h3>
      <p class="note">{words.exportBody}</p>
      <p class="note">{words.count(workspace.projects.length, workspace.profiles.length)}</p>
      {#if excluded}
        <p class="notice">{words.excluded(excluded)}</p>
        <div class="button-row">
          <button
            type="button"
            class="button"
            disabled={busy}
            onclick={() => {
              download(true);
            }}
          >
            {words.exportSaved}
          </button>
          <button
            type="button"
            class="quiet"
            disabled={busy || !workspace.bench.canSaveProject}
            onclick={() => void workspace.download()}
          >
            {text.project.save}
          </button>
        </div>
      {:else}
        <button
          type="button"
          class="button primary"
          disabled={busy}
          onclick={() => {
            download();
          }}
        >
          {words.export}
        </button>
      {/if}
    </section>
    <section>
      <h3>{words.importTitle}</h3>
      <p class="note">{words.importBody}</p>
      <button type="button" class="button" disabled={busy} onclick={() => input?.click()}>
        {words.choose}
      </button>
    </section>
  {/if}

  {#if checking || note}<p class="note" role="status">{checking ? words.checking : note}</p>{/if}
  {#if problem}<p class="notice" role="alert">{problem}</p>{/if}
</Modal>

<style>
  section {
    display: grid;
    gap: 6px;
    justify-items: start;
  }
  section + section {
    border-top: 1px solid var(--line);
    padding-top: 12px;
  }
  h3 {
    font-size: 14px;
  }
  ul {
    margin: 0;
    padding: 0 3px;
    list-style: none;
    max-height: 29dvh;
    overflow-y: auto;
    border-block: 1px solid var(--line);
  }
  li {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    padding: 7px 0;
    overflow-wrap: anywhere;
  }
  li + li {
    border-top: 1px solid var(--line);
  }
  li > span {
    color: var(--tint-text);
    font-size: 12px;
    text-align: right;
    flex: none;
  }
  .checkbox {
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
