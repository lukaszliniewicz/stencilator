<script lang="ts">
  import { untrack } from 'svelte';
  import Modal from './Modal.svelte';
  import { text } from './text';
  import { workspaceProblem, type Workspace } from './workspace.svelte';
  import { WorkspaceError, type PrinterProfile } from './workspace-store';

  let {
    workspace,
    profilesFirst,
    dismiss
  }: {
    workspace: Workspace;
    profilesFirst: boolean;
    dismiss: () => void;
  } = $props();
  const words = text.printer;
  const settings = $derived(workspace.bench.settings);
  const hasProject = $derived(!!workspace.id && !workspace.starting);
  let editing = $state(untrack(() => profilesFirst || !hasProject));
  let selection = $state(untrack(() => workspace.activeProfile?.id ?? ''));
  let customWidth = $state<number | undefined>(untrack(() => settings.bedWidthMm));
  let customDepth = $state<number | undefined>(untrack(() => settings.bedDepthMm));
  let profile = $derived(workspace.profiles.find(({ id }) => id === selection));

  let original = $state.raw<PrinterProfile>();
  let name = $state('');
  let width = $state<number>();
  let depth = $state<number>();
  let lineWidth = $state<number | null>();
  let layer = $state<number | null>();
  let nozzle = $state<number | null>();
  let busy = $state(false);
  let problem = $state('');
  let nameField: HTMLInputElement | undefined = $state();
  let selectedUpdate = $derived(
    !!profile && profile.id === workspace.activeProfile?.id && !!workspace.profileUpdate
  );

  function edit(record?: PrinterProfile): void {
    original = record;
    name = record?.name ?? '';
    width = record?.widthMm ?? settings.bedWidthMm;
    depth = record?.depthMm ?? settings.bedDepthMm;
    lineWidth = record?.lineWidthMm;
    layer = record?.layerHeightMm;
    nozzle = record?.nozzleMm;
    problem = '';
  }

  edit(untrack(() => workspace.activeProfile ?? workspace.profiles[0]));

  function apply(event: SubmitEvent): void {
    event.preventDefault();
    const w = profile?.widthMm ?? customWidth;
    const d = profile?.depthMm ?? customDepth;
    if (!w || !d || workspace.busy || workspace.bench.opening) return;
    workspace.bench.applyBed(w, d, profile?.id ?? null);
    dismiss();
  }

  async function write(action: () => Promise<void>): Promise<void> {
    busy = true;
    problem = '';
    try {
      await action();
    } catch (error) {
      problem =
        error instanceof WorkspaceError && error.code === 'invalid'
          ? words.invalid
          : workspaceProblem(error);
    } finally {
      busy = false;
    }
  }

  function saveProfile(event: SubmitEvent): void {
    event.preventDefault();
    const w = width;
    const d = depth;
    if (!name.trim() || !w || !d || busy) return;
    for (const [label, value] of [
      [words.line, lineWidth],
      [words.layer, layer],
      [words.nozzle, nozzle]
    ] as const) {
      if (typeof value === 'number' && (!Number.isFinite(value) || value <= 0)) {
        problem = words.positiveMeasurement(label);
        return;
      }
    }
    void write(async () => {
      const saved = await workspace.saveProfile(
        {
          id: original?.id ?? crypto.randomUUID(),
          name: name.trim(),
          widthMm: w,
          depthMm: d,
          ...(typeof lineWidth === 'number' ? { lineWidthMm: lineWidth } : {}),
          ...(typeof layer === 'number' ? { layerHeightMm: layer } : {}),
          ...(typeof nozzle === 'number' ? { nozzleMm: nozzle } : {})
        },
        original
      );
      edit(saved);
      if (hasProject) {
        selection = saved.id;
        editing = false;
      }
    });
  }

  function removeProfile(): void {
    const remove = original;
    if (!remove || !window.confirm(words.removeConfirm(remove.name))) return;
    void write(async () => {
      await workspace.deleteProfile(remove);
      if (selection === remove.id) selection = '';
      edit(workspace.profiles[0]);
    });
  }

  function toggleDefault(): void {
    const record = original;
    if (!record) return;
    void write(() =>
      workspace.setDefaultProfile(workspace.defaultProfile?.id === record.id ? null : record.id)
    );
  }
</script>

<Modal title={editing ? words.profiles : words.title} {dismiss} {busy}>
  {#if !editing && hasProject}
    <form class="dialog-form" onsubmit={apply}>
      <p class="note">
        {words.projectSize}:
        <strong class="mono">{text.bed.size(settings.bedWidthMm, settings.bedDepthMm)}</strong>
      </p>
      {#if workspace.profileUpdate}<p class="notice">{workspace.profileUpdate}</p>{/if}
      <label class="dialog-field">
        <span class="label">{words.profile}</span>
        <select class="field-input" bind:value={selection}>
          <option value="">{words.custom}</option>
          {#each workspace.profiles as item (item.id)}
            <option value={item.id}>{item.name}</option>
          {/each}
        </select>
      </label>
      <div class="pair">
        {#if profile}
          <label class="dialog-field">
            <span class="label">{words.width}</span>
            <input class="field-input" type="number" readonly value={profile.widthMm} />
          </label>
          <label class="dialog-field">
            <span class="label">{words.depth}</span>
            <input class="field-input" type="number" readonly value={profile.depthMm} />
          </label>
        {:else}
          <label class="dialog-field">
            <span class="label">{words.width}</span>
            <input
              class="field-input"
              type="number"
              bind:value={customWidth}
              min="50"
              max="2000"
              step="any"
              required
            />
          </label>
          <label class="dialog-field">
            <span class="label">{words.depth}</span>
            <input
              class="field-input"
              type="number"
              bind:value={customDepth}
              min="50"
              max="2000"
              step="any"
              required
            />
          </label>
        {/if}
      </div>
      {#if !profile}<p class="note">{words.customHelp}</p>{/if}
      <p class="note">{text.bed.fitNote}</p>
      <div class="button-row end">
        <button
          type="button"
          class="quiet"
          onclick={() => {
            edit(profile ?? workspace.profiles[0]);
            editing = true;
          }}
        >
          {words.manage}
        </button>
        <button
          type="submit"
          class="button primary"
          disabled={workspace.busy || workspace.bench.opening}
        >
          {selectedUpdate ? words.applyUpdate : words.apply}
        </button>
      </div>
    </form>
  {:else}
    <form class="dialog-form" onsubmit={saveProfile}>
      <div class="profile-select">
        <label class="dialog-field">
          <span class="label">{words.savedProfiles}</span>
          <select
            class="field-input"
            value={original?.id ?? ''}
            disabled={busy}
            onchange={(event) => {
              edit(workspace.profiles.find(({ id }) => id === event.currentTarget.value));
            }}
          >
            <option value="">{words.newProfile}</option>
            {#each workspace.profiles as item (item.id)}
              <option value={item.id}>{item.name}</option>
            {/each}
          </select>
        </label>
        <button
          type="button"
          class="button"
          disabled={busy}
          onclick={() => {
            edit();
            nameField?.focus();
          }}
        >
          {words.newProfile}
        </button>
      </div>
      <label class="dialog-field">
        <span class="label">{words.name}</span>
        <input
          class="field-input"
          type="text"
          bind:this={nameField}
          bind:value={name}
          maxlength="64"
          required
          disabled={busy}
        />
      </label>
      <div class="pair">
        <label class="dialog-field">
          <span class="label">{words.width}</span>
          <input
            class="field-input"
            type="number"
            bind:value={width}
            min="50"
            max="2000"
            step="any"
            required
            disabled={busy}
          />
        </label>
        <label class="dialog-field">
          <span class="label">{words.depth}</span>
          <input
            class="field-input"
            type="number"
            bind:value={depth}
            min="50"
            max="2000"
            step="any"
            required
            disabled={busy}
          />
        </label>
      </div>
      <details>
        <summary>{words.optional}</summary>
        <div class="pair">
          <label class="dialog-field">
            <span class="label">{words.line}</span>
            <input
              class="field-input"
              type="number"
              bind:value={lineWidth}
              step="any"
              disabled={busy}
            />
          </label>
          <label class="dialog-field">
            <span class="label">{words.layer}</span>
            <input
              class="field-input"
              type="number"
              bind:value={layer}
              step="any"
              disabled={busy}
            />
          </label>
          <label class="dialog-field">
            <span class="label">{words.nozzle}</span>
            <input
              class="field-input"
              type="number"
              bind:value={nozzle}
              step="any"
              disabled={busy}
            />
          </label>
        </div>
        <p class="note">{words.reference}</p>
      </details>
      {#if hasProject}<p class="note">{words.savedBedNote}</p>{/if}
      {#if original}
        <div class="button-row">
          <button type="button" class="quiet" disabled={busy} onclick={toggleDefault}>
            {workspace.defaultProfile?.id === original.id ? words.clearDefault : words.makeDefault}
          </button>
          {#if workspace.defaultProfile?.id === original.id}
            <span class="note">{words.defaultProfile}</span>
          {/if}
          <button type="button" class="quiet" disabled={busy} onclick={removeProfile}>
            {words.remove}
          </button>
        </div>
      {/if}
      <div class="button-row end">
        {#if hasProject}
          <button
            type="button"
            class="quiet"
            disabled={busy}
            onclick={() => {
              selection = workspace.activeProfile?.id ?? '';
              editing = false;
            }}
          >
            {words.back}
          </button>
        {/if}
        <button type="submit" class="button primary" disabled={busy || !name.trim()}>
          {words.save}
        </button>
      </div>
    </form>
  {/if}
  {#if problem}<p class="notice" role="alert">{problem}</p>{/if}
</Modal>

<style>
  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .profile-select {
    display: flex;
    gap: 8px;
    align-items: end;
  }
  .profile-select label {
    flex: 1;
  }
  .end {
    justify-content: space-between;
  }
  summary {
    cursor: pointer;
    font-size: 13px;
  }
  details[open] .pair,
  details p {
    margin-top: 8px;
  }
</style>
