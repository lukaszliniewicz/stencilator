<script lang="ts">
  import { onMount, tick } from 'svelte';
  import BackupDialog from './lib/BackupDialog.svelte';
  import Bench from './lib/Bench.svelte';
  import Files from './lib/Files.svelte';
  import FreeformBench from './lib/FreeformBench.svelte';
  import FreeformFiles from './lib/FreeformFiles.svelte';
  import Modal from './lib/Modal.svelte';
  import NewProject from './lib/NewProject.svelte';
  import PrintBed from './lib/PrintBed.svelte';
  import PrinterDialog from './lib/PrinterDialog.svelte';
  import ProjectBar from './lib/ProjectBar.svelte';
  import ProjectFiles from './lib/ProjectFiles.svelte';
  import SaveNotice from './lib/SaveNotice.svelte';
  import SettingsPanel from './lib/SettingsPanel.svelte';
  import Sidebar from './lib/Sidebar.svelte';
  import { text } from './lib/text';
  import { Theme, type ThemeChoice } from './lib/theme.svelte';
  import Toolbar from './lib/Toolbar.svelte';
  import { fileKind, type Preset, Workbench } from './lib/workbench.svelte';
  import { suggestedTitle, TITLE_LIMIT, Workspace } from './lib/workspace.svelte';

  type Panel = 'canvas' | 'artwork' | 'settings';
  type PickerUse = 'new' | 'replace' | 'incoming';
  type Popup =
    | 'projects'
    | 'actions'
    | 'rename'
    | 'remove'
    | 'incoming'
    | 'replace-text'
    | 'printer'
    | 'profiles'
    | 'backup';

  const theme = new Theme();
  const bench = new Workbench();
  const workspace = new Workspace(bench);
  const words = text.workspace;
  const rasterTypes = 'image/png,image/jpeg,image/webp';
  const artworkTypes = `${rasterTypes},image/svg+xml,.svg`;
  const panels = ['canvas', 'artwork', 'settings'] as const;

  let picker: HTMLInputElement | undefined = $state();
  let projectPicker: HTMLInputElement | undefined = $state();
  let pickUse: PickerUse = 'incoming';
  let dragging = $state(false);
  let chosenPreset = $state<Preset>();
  const preset = $derived(chosenPreset ?? workspace.suggestedPreset);
  let panel = $state<Panel>('canvas');
  let modal = $state<Popup>();
  let incoming = $state.raw<File>();
  let search = $state('');
  let rename = $state('');
  let main: HTMLElement | undefined = $state();
  let sizeHeading: HTMLElement | undefined = $state();
  const artworkId = $props.id();
  const projects = $derived(
    workspace.projects.filter(({ title }) =>
      title.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase())
    )
  );

  onMount(() => {
    void workspace.initialise();
    return () => {
      workspace.dispose();
    };
  });

  $effect(() => {
    const persisted = workspace.snapshot?.preferences.theme;
    if (persisted) theme.pick(persisted);
  });

  $effect(() => {
    if (bench.artworkVersion) panel = 'canvas';
  });

  function pickTheme(choice: ThemeChoice): void {
    theme.pick(choice);
    void workspace.setTheme(choice);
  }

  function show(which: Popup): void {
    modal = which;
    workspace.message = '';
    if (which === 'projects') search = '';
    if (which === 'rename') rename = workspace.title;
  }

  function dismiss(): void {
    modal = undefined;
    incoming = undefined;
  }

  function choose(use: PickerUse): void {
    if (!picker || workspace.busy || workspace.initialising || !bench.canOpen) return;
    pickUse = use;
    picker.accept =
      use === 'incoming'
        ? `${artworkTypes},.stencilator`
        : use === 'new' && preset !== 'freeform'
          ? rasterTypes
          : artworkTypes;
    picker.click();
  }

  async function chooseProject(): Promise<void> {
    dismiss();
    await tick();
    projectPicker?.click();
  }

  function receive(file: File, use: PickerUse): void {
    const kind = fileKind(file);
    if (kind === 'project' || !kind) {
      void workspace.startFile(file);
    } else if (use === 'replace' && workspace.id) {
      void workspace.replace(file);
    } else if (use === 'incoming' && workspace.id && !workspace.starting) {
      incoming = file;
      show('incoming');
    } else void workspace.startFile(file, workspace.starting ? preset : undefined);
  }

  async function chooseIncoming(replace: boolean): Promise<void> {
    const file = incoming;
    if (!file) return;
    const opened = await (replace ? workspace.replace(file) : workspace.startFile(file));
    if (opened) dismiss();
  }

  function openFiles(): void {
    bench.go('files');
    panel = 'canvas';
    void focusCanvas();
  }

  async function focusCanvas(): Promise<void> {
    panel = 'canvas';
    await tick();
    main?.focus();
  }

  function backToCanvas(): void {
    bench.go('bench');
    void focusCanvas();
  }

  async function editSize(): Promise<void> {
    bench.go('bench');
    panel = 'settings';
    await tick();
    sizeHeading?.focus();
    sizeHeading?.scrollIntoView({ block: 'nearest' });
  }

  const carriesFiles = (event: DragEvent): boolean =>
    event.dataTransfer?.types.includes('Files') ?? false;

  function paste(event: ClipboardEvent): void {
    if (
      modal ||
      event.defaultPrevented ||
      event.target instanceof HTMLInputElement ||
      event.target instanceof HTMLTextAreaElement
    )
      return;
    const file = event.clipboardData?.files[0];
    if (file) {
      event.preventDefault();
      receive(file, 'incoming');
    }
  }

  function drop(event: DragEvent): void {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    dragging = false;
    const file = event.dataTransfer?.files[0];
    if (!modal && file) receive(file, 'incoming');
  }

  function keydown(event: KeyboardEvent): void {
    if (event.defaultPrevented || modal || workspace.busy) return;
    if (event.key === 'Escape' && bench.supportDraft) {
      event.preventDefault();
      bench.cancelSupport();
      return;
    }
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
    switch (event.key.toLowerCase()) {
      case 'z':
        if (workspace.starting || event.target instanceof HTMLTextAreaElement) return;
        event.preventDefault();
        if (event.shiftKey) bench.redo();
        else bench.undo();
        break;
      case 'y':
        if (workspace.starting || event.target instanceof HTMLTextAreaElement) return;
        event.preventDefault();
        bench.redo();
        break;
      case 'o':
        event.preventDefault();
        choose('incoming');
    }
  }
</script>

<svelte:window
  onkeydown={keydown}
  onpaste={paste}
  onfocus={() => void workspace.refresh()}
  ondragenter={(event: DragEvent) => {
    if (!modal && carriesFiles(event)) dragging = true;
  }}
  ondragover={(event: DragEvent) => {
    if (carriesFiles(event)) event.preventDefault();
  }}
  ondragleave={(event: DragEvent) => {
    if (event.relatedTarget === null) dragging = false;
  }}
  ondrop={drop}
/>

<input
  bind:this={picker}
  type="file"
  accept={artworkTypes}
  aria-label={text.picture.fileLabel}
  hidden
  onchange={(event) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = '';
    if (file) receive(file, pickUse);
    pickUse = 'incoming';
  }}
/>
<input
  bind:this={projectPicker}
  type="file"
  accept=".stencilator"
  aria-label={text.project.fileLabel}
  hidden
  onchange={(event) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = '';
    if (file) void workspace.startFile(file);
  }}
/>

<div class="app">
  <ProjectBar
    {workspace}
    {theme}
    {pickTheme}
    projects={() => {
      show('projects');
    }}
    actions={() => {
      show('actions');
    }}
    files={openFiles}
    back={backToCanvas}
  />

  {#if workspace.message}<p class="problem" role="alert">{workspace.message}</p>{/if}
  {#if bench.problem}<p class="problem" role="alert">{bench.problem}</p>{/if}
  {#if workspace.starting}
    {#if workspace.storageProblem && !workspace.upgrade}
      <p class="problem" role="alert">{workspace.storageProblem}</p>
    {/if}
    {#if workspace.id || workspace.upgrade}<SaveNotice {workspace} />{/if}
    <NewProject
      {workspace}
      {preset}
      choosePreset={(choice: Preset) => {
        chosenPreset = choice;
      }}
      pick={() => {
        choose('new');
      }}
      importProject={() => void chooseProject()}
      resume={() => {
        workspace.starting = false;
        void focusCanvas();
      }}
    />
  {:else}
    <SaveNotice {workspace} />
    <div
      class="workspace"
      data-panel={panel}
      class:files={bench.screen === 'files'}
      inert={workspace.busy || bench.opening}
    >
      {#if bench.screen === 'bench'}
        <nav class="narrow-nav" aria-label={words.panelLabel}>
          {#each panels as name (name)}
            <button
              type="button"
              aria-pressed={name === panel}
              aria-controls={`${artworkId}-${name}`}
              onclick={() => {
                panel = name;
              }}
            >
              {words.panels[name]}
            </button>
          {/each}
        </nav>
      {/if}

      <aside
        class="artwork quiet-scroll"
        id={`${artworkId}-artwork`}
        aria-label={words.panels.artwork}
      >
        <Sidebar
          {bench}
          dark={theme.dark}
          choose={() => {
            choose('replace');
          }}
          typeText={() => {
            show('replace-text');
          }}
        />
      </aside>

      <main
        class="work quiet-scroll"
        id={`${artworkId}-canvas`}
        aria-label={words.panels.canvas}
        tabindex="-1"
        bind:this={main}
      >
        {#if bench.screen === 'files' && bench.canShowFiles}
          {#snippet sharedFiles()}
            <ProjectFiles
              {workspace}
              changeBed={() => {
                show('printer');
              }}
              changeSize={() => void editSize()}
            />
          {/snippet}
          {#if bench.isFreeform}
            <FreeformFiles {bench} shared={sharedFiles} />
          {:else}
            <Files {bench} dark={theme.dark} shared={sharedFiles} />
          {/if}
        {:else}
          <Toolbar {bench} />
          {#if bench.view === 'bed'}
            <PrintBed
              {bench}
              changeBed={() => {
                show('printer');
              }}
              changeSize={() => void editSize()}
            />
          {:else if bench.isFreeform}
            <FreeformBench {bench} dark={theme.dark} download={workspace.download} />
          {:else}
            <Bench {bench} dark={theme.dark} />
          {/if}
        {/if}
      </main>

      <aside
        class="settings quiet-scroll"
        id={`${artworkId}-settings`}
        aria-label={text.settings.label}
      >
        <SettingsPanel
          {bench}
          {workspace}
          bind:sizeHeading
          changeBed={() => {
            show('printer');
          }}
          manageProfiles={() => {
            show('profiles');
          }}
          placeSupport={() => void focusCanvas()}
        />
      </aside>
    </div>
  {/if}
</div>

{#if dragging}<div class="drop" aria-hidden="true">{text.freeform.dropping}</div>{/if}

{#if modal === 'projects'}
  <Modal title={words.projects} {dismiss} busy={workspace.busy}>
    <input
      type="search"
      class="field-input"
      aria-label={words.search}
      placeholder={words.search}
      bind:value={search}
      disabled={workspace.busy}
    />
    {#if projects.length}
      <ul class="project-list quiet-scroll">
        {#each projects as project (project.id)}
          <li>
            <button
              type="button"
              disabled={workspace.busy}
              onclick={async () => {
                if (await workspace.openSaved(project.id)) {
                  dismiss();
                  await focusCanvas();
                }
              }}
            >
              <strong>{project.title}</strong>
              <span class="note">
                {words.edited(text.project.modes[project.mode], project.editedAt)}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="note">{search.trim() ? words.noMatches : words.noProjects}</p>
    {/if}
    <div class="button-row">
      <button
        type="button"
        class="quiet"
        disabled={workspace.busy}
        onclick={() => void chooseProject()}
      >
        {text.project.open}
      </button>
      <button
        type="button"
        class="quiet"
        disabled={workspace.busy}
        onclick={() => {
          show('profiles');
        }}
      >
        {text.printer.manage}
      </button>
      <button
        type="button"
        class="quiet"
        disabled={workspace.busy}
        onclick={() => {
          show('backup');
        }}
      >
        {text.backup.open}
      </button>
    </div>
    <p class="note">{words.local}</p>
    {#if workspace.message}<p class="notice" role="alert">{workspace.message}</p>{/if}
  </Modal>
{:else if modal === 'actions'}
  <Modal title={words.projectActions} {dismiss} busy={workspace.busy}>
    <p><strong>{workspace.title}</strong></p>
    <div class="button-row">
      <button
        type="button"
        class="button primary"
        disabled={!bench.canSaveProject || workspace.busy}
        onclick={() => {
          dismiss();
          void workspace.download();
        }}
      >
        {text.project.save}
      </button>
      <button
        type="button"
        class="quiet"
        disabled={workspace.busy}
        onclick={() => {
          show('rename');
        }}
      >
        {words.rename}
      </button>
      <button
        type="button"
        class="quiet"
        disabled={workspace.busy || workspace.upgrade || workspace.conflict}
        onclick={async () => {
          if (await workspace.copy()) dismiss();
        }}
      >
        {words.duplicate}
      </button>
      <button
        type="button"
        class="quiet"
        disabled={!workspace.removable || workspace.busy}
        onclick={() => {
          show('remove');
        }}
      >
        {words.remove}
      </button>
    </div>
    {#if workspace.message}<p class="notice" role="alert">{workspace.message}</p>{/if}
  </Modal>
{:else if modal === 'rename'}
  <Modal title={words.renameTitle} {dismiss}>
    <form
      class="dialog-form"
      onsubmit={(event) => {
        event.preventDefault();
        workspace.rename(rename);
        dismiss();
      }}
    >
      <label class="dialog-field">
        <span class="label">{words.nameLabel}</span>
        <input
          type="text"
          class="field-input"
          required
          maxlength={TITLE_LIMIT}
          bind:value={rename}
        />
      </label>
      <div class="button-row">
        <button type="submit" class="button primary" disabled={!rename.trim()}>
          {words.saveName}
        </button>
        <button type="button" class="quiet" onclick={dismiss}>{words.cancel}</button>
      </div>
    </form>
  </Modal>
{:else if modal === 'remove'}
  <Modal title={words.removeTitle} {dismiss} busy={workspace.busy}>
    <p>{words.removeConfirm(workspace.title)}</p>
    <div class="button-row">
      <button
        type="button"
        class="button primary"
        disabled={workspace.busy}
        onclick={async () => {
          if (await workspace.remove()) dismiss();
        }}
      >
        {words.removeButton}
      </button>
      <button type="button" class="quiet" disabled={workspace.busy} onclick={dismiss}>
        {words.cancel}
      </button>
    </div>
    {#if workspace.message}<p class="notice" role="alert">{workspace.message}</p>{/if}
  </Modal>
{:else if modal === 'incoming' && incoming}
  <Modal title={words.broughtIn(suggestedTitle(incoming))} {dismiss} busy={workspace.busy}>
    <p>{words.incomingHelp(workspace.title)}</p>
    <div class="button-row">
      <button
        type="button"
        class="button primary"
        disabled={workspace.busy}
        onclick={() => void chooseIncoming(false)}
      >
        {words.separate}
      </button>
      <button
        type="button"
        class="button"
        disabled={workspace.busy}
        onclick={() => void chooseIncoming(true)}
      >
        {words.replaceNow}
      </button>
      <button type="button" class="quiet" disabled={workspace.busy} onclick={dismiss}>
        {words.cancel}
      </button>
    </div>
    {#if bench.problem}<p class="notice" role="alert">{bench.problem}</p>{/if}
  </Modal>
{:else if modal === 'replace-text'}
  <Modal title={text.freeform.type} {dismiss} busy={workspace.busy}>
    <p>{words.replaceText(workspace.title)}</p>
    <div class="button-row">
      <button
        type="button"
        class="button primary"
        disabled={workspace.busy}
        onclick={async () => {
          if (await workspace.startText(true)) dismiss();
        }}
      >
        {words.replaceNow}
      </button>
      <button type="button" class="quiet" disabled={workspace.busy} onclick={dismiss}>
        {words.cancel}
      </button>
    </div>
    {#if bench.problem}<p class="notice" role="alert">{bench.problem}</p>{/if}
  </Modal>
{:else if modal === 'printer' || modal === 'profiles'}
  <PrinterDialog {workspace} {dismiss} profilesFirst={modal === 'profiles'} />
{:else if modal === 'backup'}
  <BackupDialog {workspace} {dismiss} />
{/if}

<style>
  .app {
    min-height: 100dvh;
    height: 100dvh;
    display: flex;
    flex-direction: column;
  }

  .workspace {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template: 'artwork work settings' minmax(0, 1fr) / 245px minmax(0, 1fr) 275px;
  }

  .artwork,
  .settings,
  .work {
    min-width: 0;
    min-height: 0;
  }
  .artwork,
  .settings {
    overflow-y: auto;
  }
  .artwork {
    grid-area: artwork;
    border-right: 1px solid var(--line);
  }
  .settings {
    grid-area: settings;
    border-left: 1px solid var(--line);
  }
  .work {
    grid-area: work;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
  .files .work {
    grid-column: 2 / -1;
  }
  .files .settings {
    display: none;
  }

  .narrow-nav {
    display: none;
  }
  .problem {
    padding: 7px 16px;
    border-bottom: 1px solid var(--error-line);
    background: var(--error);
    color: var(--error-text);
    font-size: 13px;
  }

  .drop {
    position: fixed;
    z-index: 2;
    inset: 12px;
    display: grid;
    place-items: center;
    border: 2px dashed var(--tint-text);
    background: color-mix(in srgb, var(--tint) 90%, transparent);
    color: var(--tint-text);
    font: 400 28px/1.2 var(--serif);
    text-align: center;
    pointer-events: none;
  }

  .project-list {
    margin: 0;
    padding: 0 3px;
    list-style: none;
    max-height: min(46dvh, 410px);
    overflow-y: auto;
  }

  .project-list button {
    display: flex;
    width: 100%;
    justify-content: space-between;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 3px 12px;
    padding: 9px 0;
    border: 0;
    border-bottom: 1px solid var(--line);
    background: none;
    text-align: left;
    overflow-wrap: anywhere;
  }

  .project-list button:hover {
    color: var(--tint-text);
  }

  @media (max-width: 1100px) {
    .workspace {
      grid-template: 'nav' auto 'panel' minmax(0, 1fr) / minmax(0, 1fr);
    }
    .workspace.files {
      grid-template: 'panel' minmax(0, 1fr) / minmax(0, 1fr);
    }
    .narrow-nav {
      grid-area: nav;
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
      padding: 4px 10px;
      border-bottom: 1px solid var(--line);
    }
    .narrow-nav button {
      min-height: 38px;
      padding: 0 8px;
      background: var(--panel);
      border: 1px solid var(--line);
      font-size: 12px;
    }
    .narrow-nav button[aria-pressed='true'] {
      color: var(--tint-text);
      background: var(--tint);
      border-color: var(--tint-line);
    }
    .artwork,
    .settings,
    .work {
      grid-area: panel;
      border: 0;
    }
    .files .work {
      grid-column: 1;
    }
    .artwork,
    .settings {
      width: min(100%, 620px);
      justify-self: center;
    }
    .workspace:not([data-panel='artwork']) .artwork,
    .workspace:not([data-panel='settings']) .settings,
    .workspace:not([data-panel='canvas']) .work,
    .files :is(.artwork, .settings) {
      display: none;
    }
  }
</style>
