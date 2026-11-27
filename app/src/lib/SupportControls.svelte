<script lang="ts">
  import NumberField from './NumberField.svelte';
  import Segmented from './Segmented.svelte';
  import type { Settings } from './settings';
  import { text } from './text';
  import type { Workbench } from './workbench.svelte';

  let { bench }: { bench: Workbench } = $props();
  const words = text.supports;
  const ends = [0, 1] as const;
  const choices = (['none', 'flat', 'raised'] as const).map((value) => ({
    value,
    label: words.kinds[value]
  }));
  const kind = $derived(bench.supportKind);
  const raised = $derived(kind === 'raised');
  const plate = $derived(bench.selectedPlate);
  const selected = $derived(bench.selectedSupport);
  const draft = $derived(bench.supportDraft);
  const supports = $derived(bench.drawing?.supports ?? []);
  const hasEdits = $derived(
    bench.settings.supportEdits.some((edit) => edit.paint === plate?.colour && edit.kind === kind)
  );
  const missingPaintEdits = $derived(
    bench.settings.supportEdits.filter(
      (edit) => edit.kind === kind && !bench.freeform?.plates.some((p) => p.colour === edit.paint)
    ).length
  );
  const set =
    (
      key: keyof Pick<
        Settings,
        'connectorWidthMm' | 'connectorClearanceMm' | 'bridgeWidthMm' | 'beamMm' | 'warnSpanMm'
      >
    ) =>
    (value: number): void => {
      bench.change(key, value);
    };
</script>

<Segmented
  label={words.heading}
  options={choices}
  value={kind}
  onpick={bench.setSupportKind}
  stretch
/>
{#if kind !== 'none'}
  <p class="note">{words.note}</p>
  <div class="fields">
    {#if raised}
      <NumberField
        label={words.width}
        value={bench.settings.connectorWidthMm}
        step={0.1}
        onchange={set('connectorWidthMm')}
      />
      <NumberField
        label={words.clearance}
        value={bench.settings.connectorClearanceMm}
        step={0.1}
        onchange={set('connectorClearanceMm')}
      />
    {:else}
      <NumberField
        label={words.width}
        value={bench.settings.bridgeWidthMm}
        step={0.1}
        onchange={set('bridgeWidthMm')}
      />
    {/if}
  </div>
  {#if raised}
    <details>
      <summary>{words.beamSettings}</summary>
      <div class="fields">
        <NumberField
          label={words.beam}
          value={bench.settings.beamMm}
          step={0.1}
          onchange={set('beamMm')}
        />
        <NumberField
          label={words.warn}
          value={bench.settings.warnSpanMm}
          onchange={set('warnSpanMm')}
        />
      </div>
    </details>
    <p class="note">{words.trial}</p>
  {/if}
  {#if bench.current && missingPaintEdits}
    <p class="notice">{words.missingPaints(missingPaintEdits)}</p>
  {/if}
  {#if plate}
    <div class="editor">
      <h3 class="label">{words.onPlate(plate.paint)}</h3>
      {#if supports.length}
        <ul>
          {#each supports as support, i (support)}
            <li>
              <button
                type="button"
                class="support"
                disabled={!bench.ready || !!draft}
                aria-pressed={selected === support}
                onclick={() => {
                  bench.selectSupport(support);
                }}
              >
                <strong>{words.item(i, raised)}</strong>
                <span class:invalid={!!support.issue} class="note">
                  {support.issue === 'suggestion'
                    ? words.oldRemoval
                    : support.issue
                      ? words.unplaced(1)
                      : words.span(support.span_mm)}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {:else if bench.drawing}
        <p class="note">{words.none}</p>
      {/if}
      {#if draft}
        <p class="note">{words.placement}</p>
        <div class="fields">
          {#each ends as end (end)}
            <NumberField
              label={words.coordinate(end, 'X')}
              value={draft.ends[end][0] / 1000}
              step={0.1}
              disabled={bench.checkingSupport}
              onchange={(value: number) => {
                bench.setSupportCoordinate(end, 0, value);
              }}
            />
            <NumberField
              label={words.coordinate(end, 'Y')}
              value={draft.ends[end][1] / 1000}
              step={0.1}
              disabled={bench.checkingSupport}
              onchange={(value: number) => {
                bench.setSupportCoordinate(end, 1, value);
              }}
            />
          {/each}
        </div>
        <div class="actions">
          <button
            type="button"
            class="button primary"
            disabled={bench.checkingSupport || !bench.ready}
            onclick={bench.applySupport}
          >
            {bench.checkingSupport ? words.checking : words.apply}
          </button>
          <button type="button" class="button" onclick={bench.cancelSupport}>{words.cancel}</button>
        </div>
      {:else}
        {#if selected}
          <div class="actions">
            {#if selected.issue !== 'suggestion'}
              <button
                type="button"
                class="button"
                disabled={!bench.ready}
                onclick={() => {
                  bench.editSupport(selected);
                }}
              >
                {words.move}
              </button>
            {/if}
            <button
              type="button"
              class="button"
              disabled={!bench.ready}
              onclick={() => {
                bench.removeSupport(selected);
              }}
            >
              {selected.issue === 'suggestion' ? words.discardRemoval : words.remove}
            </button>
          </div>
          {#if selected.issue}<p class="notice">{words.issues[selected.issue]}</p>{/if}
        {/if}
        <div class="actions">
          <button
            type="button"
            class="button primary"
            disabled={!bench.ready}
            onclick={() => {
              bench.editSupport();
            }}
          >
            {words.add}
          </button>
          {#if hasEdits}
            <button
              type="button"
              class="button"
              disabled={!bench.ready}
              onclick={bench.restoreSupports}
            >
              {words.restore}
            </button>
          {/if}
        </div>
      {/if}
      {#if bench.supportProblem}
        <p class="notice" role="alert">{bench.supportProblem}</p>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .editor {
    border-top: 1px solid var(--line);
    padding-top: 12px;
    display: grid;
    gap: 10px;
  }

  .fields {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    align-items: end;
  }

  summary {
    cursor: pointer;
    font-size: 13px;
  }

  details[open] .fields {
    margin-top: 10px;
  }

  ul {
    display: grid;
    gap: 4px;
    max-height: 198px;
    overflow: auto;
    list-style: none;
    margin: 0;
    padding: 2px;
  }

  .support {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 4px 8px;
    width: 100%;
    text-align: left;
    padding: 7px 8px;
    border: 1px solid var(--line);
    background: var(--surface);
    font-size: 12px;
  }

  .support[aria-pressed='true'] {
    background: var(--tint);
    border-color: var(--tint-line);
  }

  .invalid {
    color: var(--warn-text);
  }

  .actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .button {
    min-height: 34px;
    padding-inline: 8px;
    font-size: 11px;
  }
</style>
