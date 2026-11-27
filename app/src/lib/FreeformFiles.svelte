<script lang="ts">
  import { platesFileName, stencilFileName } from './names';
  import PlateIssues from './PlateIssues.svelte';
  import { text } from './text';
  import type { Workbench } from './workbench.svelte';

  let { bench }: { bench: Workbench } = $props();
  const words = text.freeform;
  const plates = $derived(bench.freeform?.plates ?? []);
  const frame = $derived(bench.freeform?.frame);
  const thickness = $derived(bench.freeform?.thicknessMm ?? 0);
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
    {/if}
    {#if bench.progress}
      <p class="note" role="status">
        {bench.progress === 'paints' ? words.finding : words.building}
      </p>
    {/if}
  </header>

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

  {#if plates.length}
    <section aria-labelledby="plate-files-heading">
      <h2 id="plate-files-heading" class="group">{words.parts}</h2>
      {#if frame}
        {@const name = stencilFileName(bench.base, frame.file)}
        <div class="frame">
          <div class="about">
            <strong>{text.registration.heading}</strong>
            <p class="note">
              {words.partSize(frame.width_mm, frame.height_mm, thickness)} ·
              {frame.fits_bed ? text.registration.fits : text.registration.bedWarning}
            </p>
          </div>
          <button type="button" class="button" disabled={!bench.ready} onclick={bench.saveFrame}>
            {bench.saving === name ? text.files.saving : text.registration.save}
          </button>
        </div>
      {/if}
      <ul class="plates">
        {#each plates as plate, i (plate.stencil)}
          {@const name = stencilFileName(bench.base, plate.stencil)}
          <li>
            <span class="label">{text.coating.number(i + 1)}</span>
            <p class="title">
              <span class="swatch" style:background={plate.colour} aria-hidden="true"></span>
              <strong>{words.plateFor(plate.paint, plate.colour)}</strong>
            </p>
            <p class="note">
              {words.partSize(
                plate.width_mm,
                plate.height_mm,
                plate.height_with_supports_mm ?? thickness
              )}
              · {words.openings(plate.openings)}
            </p>
            {#if plate.ral}<p class="note">{text.finishing.ral(plate.ral.code)}</p>{/if}
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
  {/if}

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

  .formats li,
  .frame {
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

  .project {
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

  @media (max-width: 720px) {
    .files {
      padding: 24px 16px;
    }
  }
</style>
