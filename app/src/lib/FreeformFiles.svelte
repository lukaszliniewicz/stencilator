<script lang="ts">
  import { platesFileName, stencilFileName } from './names';
  import PlateIssues from './PlateIssues.svelte';
  import { text } from './text';
  import type { Workbench } from './workbench.svelte';

  let { bench }: { bench: Workbench } = $props();
  const words = text.freeform;
  const plates = $derived(bench.freeform?.plates ?? []);
  const frame = $derived(bench.freeform?.frame);
  const FORMATS = [
    { format: '3mf', name: words.threeMf, description: words.threeMfBody },
    { format: 'zip', name: words.zip, description: words.zipBody }
  ] as const;
</script>

<div class="files">
  <header class="top">
    <h1>{text.files.heading}</h1>
    <p class="note">{bench.fileName || words.typed} · {words.allPlates(plates.length)}</p>
    {#if frame}
      <p class="note">{words.includedFrame(frame.width_mm, frame.height_mm)}</p>
      {#if !frame.fits_bed}<p class="notice">{words.frameBedWarning}</p>{/if}
    {/if}
    {#if bench.progress}
      <p class="note" role="status">
        {bench.progress === 'paints' ? words.finding : words.building}
      </p>
    {/if}
  </header>

  <div class="columns">
    <div class="column">
      <section aria-labelledby="print-files-heading">
        <h2 id="print-files-heading" class="group">{words.forPrinting}</h2>
        {#if plates.length}
          <ul class="formats">
            {#each FORMATS as option (option.format)}
              {@const name = platesFileName(bench.base, option.format)}
              <li class:recommended={option.format === '3mf'}>
                <div class="about">
                  <p class="title">
                    <strong>{option.name}</strong>
                    {#if option.format === '3mf'}<span class="label badge">
                        {text.files.best}
                      </span>{/if}
                  </p>
                  <p class="note">{option.description}</p>
                  <p class="mono filename">{name}</p>
                </div>
                <button
                  type="button"
                  class="button"
                  class:primary={option.format === '3mf'}
                  disabled={!bench.ready}
                  onclick={() => void bench.savePlates(option.format)}
                >
                  {bench.saving === name ? text.files.saving : text.files.save(option.name)}
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="notice">{words.noneTicked}</p>
        {/if}
      </section>

      <section class="project" aria-labelledby="project-files-heading">
        <h2 id="project-files-heading" class="group">{text.project.heading}</h2>
        <p class="note">{text.project.body}</p>
        <p class="mono filename">{bench.projectName}</p>
        <button
          type="button"
          class="button"
          disabled={!bench.canSaveProject}
          onclick={bench.saveProject}
        >
          {bench.saving === bench.projectName ? text.files.saving : text.project.save}
        </button>
      </section>
    </div>

    {#if plates.length}
      <div class="column">
        <section aria-labelledby="plate-files-heading">
          <h2 id="plate-files-heading" class="group">{words.plates}</h2>
          <ul class="plates">
            {#each plates as plate, i (plate.stencil)}
              {@const name = stencilFileName(bench.base, plate.stencil)}
              <li>
                <p class="title">
                  <span class="swatch" style:background={plate.colour} aria-hidden="true"></span>
                  <strong>{words.plateFor(plate.paint, plate.colour)}</strong>
                </p>
                <p class="note">
                  {words.artworkSize(plate.width_mm, plate.height_mm)} · {words.openings(
                    plate.openings
                  )}
                </p>
                <PlateIssues {plate} />
                <button
                  type="button"
                  class="button small"
                  disabled={!bench.ready}
                  aria-label={words.savePlate(plate.paint, plate.colour)}
                  onclick={() => void bench.savePlate(i)}
                >
                  {bench.saving === name ? text.files.saving : text.files.saveStl}
                </button>
              </li>
            {/each}
          </ul>
        </section>

        <section class="advice" aria-labelledby="freeform-advice-heading">
          <h2 id="freeform-advice-heading" class="label">{text.files.beforeYouPrint}</h2>
          <ul class="note">
            <li>{words.inspect}</li>
            {#if plates.some((p) => p.islands.length)}<li>{words.supportIslands}</li>{/if}
            {#if plates.some((p) => p.thin_webs.length)}<li>{words.inspectThinWebs}</li>{/if}
            {#if plates.some((p) => p.narrow_openings.length)}<li>
                {words.inspectNarrowOpenings}
              </li>{/if}
            {#if plates.some((p) => !p.fits_bed)}<li>{words.resize}</li>{/if}
          </ul>
        </section>
      </div>
    {/if}
  </div>
</div>

<style>
  .files {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 28px 32px 40px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .top {
    display: grid;
    gap: 4px;
  }

  h1 {
    font: 400 34px/1.1 var(--serif);
    letter-spacing: -0.02em;
  }

  .columns {
    display: flex;
    flex-wrap: wrap;
    gap: 28px;
    align-items: flex-start;
  }

  .column {
    flex: 1 1 280px;
    min-width: 0;
    display: grid;
    gap: 24px;
  }

  section {
    display: grid;
    gap: 10px;
  }

  .group {
    font-size: 22px;
  }

  .formats,
  .plates {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .formats {
    display: grid;
    gap: 8px;
  }

  .formats li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--line);
  }

  .formats li.recommended {
    background: var(--surface);
  }

  .about {
    flex: 1 1 150px;
    min-width: 0;
    display: grid;
    gap: 3px;
  }

  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .title strong {
    font-weight: 600;
  }

  .badge {
    padding: 1px 4px;
    border: 1px solid var(--tint-line);
    background: var(--tint);
    color: var(--tint-text);
  }

  .filename {
    overflow-wrap: anywhere;
    color: var(--text-3);
    font-size: 12px;
  }

  .project,
  .advice {
    border-top: 1px solid var(--line);
    padding-top: 12px;
  }

  .project button,
  .small {
    justify-self: start;
  }

  .plates {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 174px), 1fr));
    gap: 8px;
  }

  .plates li {
    display: grid;
    align-content: start;
    gap: 7px;
    padding: 12px;
    border: 1px solid var(--line);
  }

  .swatch {
    width: 14px;
    height: 14px;
    border: 1px solid var(--line-strong);
  }

  .small {
    min-height: 34px;
    font-size: 11px;
  }

  .advice ul {
    margin: 0;
    padding-left: 18px;
    display: grid;
    gap: 5px;
  }

  @media (max-width: 720px) {
    .files {
      padding: 24px 16px;
    }
  }
</style>
