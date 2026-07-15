<script lang="ts">
  import type { ThemePreference } from "$lib/api";

  interface Props {
    value: ThemePreference;
    onchange: (value: ThemePreference) => void;
  }

  let { value, onchange }: Props = $props();

  const choices: ReadonlyArray<{
    value: ThemePreference;
    label: string;
    detail: string;
  }> = [
    { value: "system", label: "System", detail: "Follow OS" },
    { value: "light", label: "Light", detail: "Always light" },
    { value: "dark", label: "Dark", detail: "Always dark" },
  ];
</script>

<fieldset class="appearance-section">
  <legend>Appearance</legend>
  <div class="theme-options" role="radiogroup" aria-label="Application theme">
    {#each choices as choice (choice.value)}
      <label class:checked={value === choice.value}>
        <input
          type="radio"
          name="application-theme"
          value={choice.value}
          aria-label={choice.label}
          checked={value === choice.value}
          onchange={() => onchange(choice.value)}
        />
        <span
          class="preview"
          class:system={choice.value === "system"}
          class:light={choice.value === "light"}
          class:dark={choice.value === "dark"}
          aria-hidden="true"
        >
          <span></span><span></span><span></span>
        </span>
        <span class="copy">
          <strong>{choice.label}</strong>
          <small>{choice.detail}</small>
        </span>
      </label>
    {/each}
  </div>
</fieldset>

<style>
  .appearance-section {
    padding-bottom: 11px;
  }

  .theme-options {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  label {
    display: grid;
    grid-template-columns: auto 34px minmax(0, 1fr);
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 7px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-1);
    cursor: pointer;
  }

  label:hover {
    background: var(--bg-3);
    border-color: var(--border-strong);
  }

  label.checked {
    background: var(--accent-subtle);
    border-color: var(--accent-dim);
  }

  input {
    appearance: none;
    width: 14px;
    height: 14px;
    margin: 0;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    background: var(--bg-0);
    outline: none;
  }

  input:checked {
    border: 4px solid var(--accent-dim);
  }

  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .preview {
    display: grid;
    grid-template-rows: 5px 1fr 1fr;
    gap: 2px;
    width: 34px;
    height: 26px;
    padding: 3px;
    border: 1px solid #8c959f;
    border-radius: 4px;
    background: #ffffff;
  }

  .preview > span {
    border-radius: 1px;
    background: #d0d7de;
  }

  .preview > span:first-child {
    background: #1f883d;
  }

  .preview.dark {
    border-color: #57606a;
    background: #0d1117;
  }

  .preview.dark > span {
    background: #30363d;
  }

  .preview.dark > span:first-child {
    background: #3fb950;
  }

  .preview.system {
    background: linear-gradient(90deg, #ffffff 0 50%, #0d1117 50% 100%);
  }

  .preview.system > span {
    background: linear-gradient(90deg, #d0d7de 0 50%, #30363d 50% 100%);
  }

  .preview.system > span:first-child {
    background: linear-gradient(90deg, #1f883d 0 50%, #3fb950 50% 100%);
  }

  .copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
    line-height: 1.2;
  }

  strong {
    color: var(--text-0);
    font-size: 12px;
    font-weight: 500;
  }

  small {
    overflow: hidden;
    color: var(--text-2);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
