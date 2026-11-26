<script lang="ts">
  // Artwork and paints on the left, the bench or files in the middle, settings on the right.
  import Bench from './lib/Bench.svelte';
  import Files from './lib/Files.svelte';
  import FreeformBench from './lib/FreeformBench.svelte';
  import FreeformFiles from './lib/FreeformFiles.svelte';
  import SettingsPanel from './lib/SettingsPanel.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import { text } from './lib/text';
  import { Theme } from './lib/theme.svelte';
  import Toolbar from './lib/Toolbar.svelte';
  import { Workbench } from './lib/workbench.svelte';

  const theme = new Theme();
  const bench = new Workbench();

  let picker: HTMLInputElement | undefined = $state();
  let projectPicker: HTMLInputElement | undefined = $state();
  let dragging = $state(false);

  const choose = (): void => {
    if (bench.canOpen) picker?.click();
  };

  const openFirst = (files: FileList | null | undefined): void => {
    const file = files?.[0];
    if (file) void bench.open(file);
  };

  const carriesFiles = (event: DragEvent): boolean =>
    event.dataTransfer?.types.includes('Files') ?? false;

  function paste(event: ClipboardEvent): void {
    if (
      event.defaultPrevented ||
      event.target instanceof HTMLInputElement ||
      event.target instanceof HTMLTextAreaElement
    )
      return;
    openFirst(event.clipboardData?.files);
  }

  function dragenter(event: DragEvent): void {
    if (carriesFiles(event)) dragging = true;
  }

  function dragover(event: DragEvent): void {
    if (carriesFiles(event)) event.preventDefault();
  }

  function dragleave(event: DragEvent): void {
    // Only leaving the window itself ends the drag; moving between elements does not.
    if (event.relatedTarget === null) dragging = false;
  }

  function drop(event: DragEvent): void {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    dragging = false;
    openFirst(event.dataTransfer?.files);
  }

  function keydown(event: KeyboardEvent): void {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
    const key = event.key.toLowerCase();
    if (key === 'z') {
      event.preventDefault();
      if (event.shiftKey) bench.redo();
      else bench.undo();
    } else if (key === 'y') {
      event.preventDefault();
      bench.redo();
    } else if (key === 'o') {
      event.preventDefault();
      choose();
    }
  }
</script>

<svelte:window
  onkeydown={keydown}
  onpaste={paste}
  ondragenter={dragenter}
  ondragover={dragover}
  ondragleave={dragleave}
  ondrop={drop}
/>

<input
  bind:this={picker}
  type="file"
  accept="image/png,image/jpeg,image/webp,image/svg+xml,.svg"
  aria-label={text.picture.fileLabel}
  hidden
  onchange={(event) => {
    openFirst(event.currentTarget.files);
    event.currentTarget.value = '';
  }}
/>

<input
  bind:this={projectPicker}
  type="file"
  accept=".stencilator"
  aria-label={text.project.fileLabel}
  hidden
  onchange={(event) => {
    openFirst(event.currentTarget.files);
    event.currentTarget.value = '';
  }}
/>

<div class="app">
  <aside class="side" aria-label={bench.isFreeform ? text.freeform.artwork : text.picture.heading}>
    <Sidebar {bench} {theme} {choose} chooseProject={() => projectPicker?.click()} />
  </aside>

  <main class="work">
    <Toolbar {bench} />
    {#if bench.problem}
      <p class="problem" role="alert">{bench.problem}</p>
    {/if}
    {#if bench.screen === 'files' && bench.canShowFiles}
      {#if bench.isFreeform}
        <FreeformFiles {bench} />
      {:else}
        <Files {bench} dark={theme.dark} />
      {/if}
    {:else if bench.isFreeform}
      <FreeformBench {bench} {choose} />
    {:else}
      <Bench {bench} dark={theme.dark} {choose} />
    {/if}
    {#if dragging}
      <div class="drop" aria-hidden="true"><p>{text.freeform.dropping}</p></div>
    {/if}
  </main>

  <aside class="settings" aria-label={text.settings.label}>
    <SettingsPanel {bench} />
  </aside>
</div>

<style>
  .app {
    height: 100dvh;
    display: grid;
    grid-template-columns: 288px minmax(0, 1fr) 300px;
  }

  .side,
  .settings {
    min-height: 0;
    overflow-y: auto;
  }

  .work {
    position: relative;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-inline: 1px solid var(--line);
  }

  .problem {
    padding: 10px 16px;
    border-bottom: 1px solid var(--error-line);
    background: var(--error);
    color: var(--error-text);
    font-size: 13px;
  }

  .drop {
    position: absolute;
    inset: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--tint-text);
    background: color-mix(in srgb, var(--tint) 85%, transparent);
    color: var(--tint-text);
    font-family: var(--serif);
    font-size: 28px;
    pointer-events: none;
  }

  @media (max-width: 1099px) {
    .app {
      height: auto;
      display: flex;
      flex-direction: column;
    }

    .side,
    .settings {
      overflow: visible;
    }

    .work {
      height: 85dvh;
      border-inline: 0;
      border-block: 1px solid var(--line);
    }
  }
</style>
