<script lang="ts" generics="Value extends string">
  interface Choice {
    value: Value;
    label: string;
  }

  interface Props {
    label: string;
    ariaLabel: string;
    name: string;
    value: Value;
    options: readonly Choice[];
    onchange: (value: Value) => void;
    disabled?: boolean;
    grow?: boolean;
  }

  let {
    label,
    ariaLabel,
    name,
    value,
    options,
    onchange,
    disabled = false,
    grow = false,
  }: Props = $props();
</script>

<div class="choice-group" class:grow>
  <span>{label}</span>
  <div class="options" role="radiogroup" aria-label={ariaLabel}>
    {#each options as option (option.value)}
      <label>
        <input
          type="radio"
          {name}
          value={option.value}
          checked={value === option.value}
          {disabled}
          onchange={() => onchange(option.value)}
        />
        <span>{option.label}</span>
      </label>
    {/each}
  </div>
</div>

<style>
  .choice-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .choice-group > span {
    font-size: 11px;
    color: var(--text-1);
  }

  .grow {
    flex: 1;
  }

  .options {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 30px;
  }

  label {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    white-space: nowrap;
  }

  label:has(input:checked) {
    border-color: var(--accent-dim);
    background: var(--bg-3);
  }

  input {
    margin: 0;
  }
</style>
