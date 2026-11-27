<script lang="ts">
  import { printParts } from './print-parts';
  import { text } from './text';
  import type { Workspace } from './workspace.svelte';

  let {
    workspace,
    changeBed,
    changeSize
  }: {
    workspace: Workspace;
    changeBed: () => void;
    changeSize: () => void;
  } = $props();
  const words = text.bed;
  const bench = $derived(workspace.bench);
  const parts = $derived(printParts(bench));
  const warnings = $derived(parts?.filter(({ fit }) => fit !== 'as-drawn') ?? []);
  const tooLarge = $derived(warnings.some(({ fit }) => fit === 'too-large'));
  const { bedWidthMm, bedDepthMm } = $derived(bench.settings);
</script>

<div class="sections">
  <section class="box" aria-label={text.project.heading}>
    <div>
      <h2 class="group">{text.project.heading}</h2>
      <p class="note">{text.project.body}</p>
      <p class="note mono">{workspace.downloadName}</p>
    </div>
    <button
      type="button"
      class="button"
      disabled={!bench.canSaveProject}
      onclick={() => void workspace.download()}
    >
      {bench.saving === workspace.downloadName ? text.files.saving : text.project.save}
    </button>
  </section>

  <section class="box" aria-label={words.findings}>
    <div>
      <h2 class="group">{words.findings}</h2>
      <p class="note">
        {workspace.activeProfile?.name ?? text.printer.custom} ·
        <span class="mono">{words.size(bedWidthMm, bedDepthMm)}</span>
      </p>
      {#if workspace.profileUpdate}<p class="notice">{workspace.profileUpdate}</p>{/if}
    </div>
    <div class="button-row">
      {#if tooLarge}<button class="quiet" type="button" onclick={changeSize}>
          {words.resize}
        </button>{/if}
      <button class="quiet" type="button" onclick={changeBed}>{text.printer.change}</button>
    </div>
    {#if !bench.current || !bench.valid}
      <p class="note wide">{bench.current ? words.unavailable : words.checking}</p>
    {:else if parts === null}
      <p class="notice wide">{words.invalidBed}</p>
    {:else if warnings.length}
      <ul class="notice wide">
        {#each warnings as part (part.id)}
          <li>
            <strong>{part.name}:</strong>
            {words.fit[part.fit]} · {words.size(part.width, part.depth)}
            {#if part.fit === 'quarter-turn'}
              <span class="note">
                {words.clearance(bedWidthMm - part.depth, bedDepthMm - part.width)}
              </span>
            {:else}
              <span class="note">
                {words.asDrawnOver(
                  words.overflow(part.width - bedWidthMm, part.depth - bedDepthMm)
                )}
                {words.turnedOver(words.overflow(part.depth - bedWidthMm, part.width - bedDepthMm))}
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <p class="note wide">{parts.length ? words.allFit : words.none}</p>
    {/if}
    <p class="note wide">{words.fitNote}</p>
    {#if bench.isFreeform && bench.freeform?.plates.length}<p class="note wide">
        {words.placement3mf}
      </p>{/if}
  </section>
</div>

<style>
  .sections {
    display: grid;
    gap: 8px;
  }
  .box {
    display: flex;
    flex-wrap: wrap;
    align-items: start;
    justify-content: space-between;
    gap: 6px 14px;
    border: 1px solid var(--line);
    background: var(--surface);
    padding: 10px 12px;
    overflow-wrap: anywhere;
  }
  .box > div:first-child {
    flex: 1 1 220px;
  }
  .wide {
    width: 100%;
  }
  ul {
    margin: 0;
    padding-inline-start: 26px;
  }
  li > span {
    display: block;
  }
</style>
