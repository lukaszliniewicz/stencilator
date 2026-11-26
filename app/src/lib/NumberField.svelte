<script lang="ts">
  // A labelled number. An empty or unreadable entry is passed on as NaN, and the engine
  // says what it needs instead.
  interface Props {
    label: string;
    value: number;
    step?: number;
    disabled?: boolean;
    onchange: (value: number) => void;
  }

  let { label, value, step = 1, disabled = false, onchange }: Props = $props();

  const get = (): number => value;
  const set = (next: unknown): void => {
    onchange(typeof next === 'number' ? next : Number.NaN);
  };
</script>

<label class="field">
  <span class="label">{label}</span>
  <input type="number" inputmode="decimal" {step} {disabled} bind:value={get, set} />
</label>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  input {
    width: 100%;
    height: 40px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: 0;
    background: var(--field);
    font-variant-numeric: tabular-nums;
  }

  input:hover:not(:disabled) {
    border-color: var(--text-3);
  }

  input:disabled {
    color: var(--text-3);
  }
</style>
