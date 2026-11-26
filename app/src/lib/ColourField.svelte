<script lang="ts">
  // A labelled colour with its hex value beside the swatch.
  interface Props {
    label: string;
    value: string;
    onchange: (value: string) => void;
  }

  let { label, value, onchange }: Props = $props();
</script>

<label class="field">
  <span class="label">{label}</span>
  <span class="value">
    <input
      type="color"
      {value}
      oninput={(event) => {
        onchange(event.currentTarget.value);
      }}
    />
    <span class="mono">{value.toUpperCase()}</span>
  </span>
</label>

<style>
  .field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-height: 40px;
  }

  .value {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }

  input {
    width: 36px;
    height: 32px;
    padding: 2px;
    border: 1px solid var(--line-strong);
    background: var(--field);
  }
</style>
