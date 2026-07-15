<script lang="ts">
  import type { DirectoryComparisonSummary } from "$lib/directory-comparison";

  interface Props {
    active: boolean;
    summary: DirectoryComparisonSummary;
    differencesOnly: boolean;
    ontoggle: () => void;
    onfilterchange: (checked: boolean) => void;
  }

  let { active, summary, differencesOnly, ontoggle, onfilterchange }: Props = $props();
</script>

<div class="comparison-bar" data-testid="directory-comparison-bar">
  <button
    class="compare-toggle"
    class:active
    aria-label={active ? "Stop Comparing Folders" : "Compare Folders"}
    aria-pressed={active}
    onclick={ontoggle}
  >
    <span aria-hidden="true">⇄</span>
    <span>Compare</span>
  </button>

  {#if active}
    <div
      class="summary"
      role="status"
      aria-live="polite"
      data-testid="directory-comparison-summary"
      data-local-only={summary.localOnly}
      data-different={summary.different}
      data-remote-only={summary.remoteOnly}
      data-matching={summary.matching}
    >
      <span class="result local-only">
        <span class="signal" aria-hidden="true"></span>{summary.localOnly} Local Only
      </span>
      <span class="result different">
        <span class="signal" aria-hidden="true"></span>{summary.different} Different
      </span>
      <span class="result remote-only">
        <span class="signal" aria-hidden="true"></span>{summary.remoteOnly} Remote Only
      </span>
      <span class="result matching">
        <span class="signal" aria-hidden="true"></span>{summary.matching} Same Metadata
      </span>
    </div>

    <span
      class="scope"
      title="Compares names, types, sizes, and known modification times without scanning directory contents"
    >Current Folders · Metadata Only</span>
    <label class="filter">
      <input
        type="checkbox"
        checked={differencesOnly}
        onchange={(event) => onfilterchange(event.currentTarget.checked)}
      />
      <span>Differences Only</span>
    </label>
  {/if}
</div>

<style>
  .comparison-bar {
    min-height: 36px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 8px;
    margin: 8px 8px 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-1);
    overflow: hidden;
  }

  .compare-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    padding: 3px 9px;
    color: var(--text-1);
    font-size: 12px;
  }

  .compare-toggle.active {
    color: var(--text-0);
    border-color: var(--accent);
    background: var(--accent-subtle);
  }

  .compare-toggle:focus-visible,
  .filter:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .summary {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
    flex: 1;
    min-width: 0;
  }

  .result {
    --signal: var(--text-2);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    color: var(--text-1);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .signal {
    width: 7px;
    height: 7px;
    border: 2px solid var(--signal);
    border-radius: 50%;
  }

  .local-only {
    --signal: var(--compare-local);
  }

  .different {
    --signal: var(--compare-different);
  }

  .remote-only {
    --signal: var(--compare-remote);
  }

  .scope {
    flex: none;
    color: var(--text-1);
    font-size: 10px;
    white-space: nowrap;
  }

  .filter {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    padding: 3px 4px;
    border-radius: var(--radius);
    color: var(--text-1);
    font-size: 11px;
    white-space: nowrap;
    cursor: pointer;
  }

  .filter:hover {
    color: var(--text-0);
    background: var(--bg-3);
  }

  .filter input {
    margin: 0;
    accent-color: var(--accent);
  }

  @media (max-width: 980px) {
    .scope {
      display: none;
    }
  }
</style>
