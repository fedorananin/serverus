<script lang="ts">
  import type { DirectoryComparisonStatus } from "$lib/directory-comparison";

  interface Props {
    status?: DirectoryComparisonStatus;
    id?: string;
  }

  let { status, id }: Props = $props();

  const marker = $derived.by(() => {
    switch (status) {
      case "local-only":
        return { label: "Local Only", text: "L" };
      case "remote-only":
        return { label: "Remote Only", text: "R" };
      case "different":
        return { label: "Different", text: "≠" };
      default:
        return null;
    }
  });
</script>

{#if marker}
  <span {id} class="marker {status}" title={marker.label}>
    <span aria-hidden="true">{marker.text}</span>
    <span class="sr-only">{marker.label}</span>
  </span>
{:else if status === "matching"}
  <span {id} class="sr-only">Same Metadata</span>
{/if}

<style>
  .marker {
    --marker: var(--text-2);
    --marker-bg: var(--bg-3);
    display: inline-grid;
    place-items: center;
    width: 15px;
    height: 15px;
    flex: none;
    border: 1px solid var(--marker);
    border-radius: 4px;
    background: var(--marker-bg);
    color: var(--marker);
    font-family: var(--font-ui);
    font-size: 9px;
    font-weight: 700;
    line-height: 1;
  }

  .local-only {
    --marker: var(--compare-local);
    --marker-bg: var(--compare-local-subtle);
  }

  .remote-only {
    --marker: var(--compare-remote);
    --marker-bg: var(--compare-remote-subtle);
  }

  .different {
    --marker: var(--compare-different);
    --marker-bg: var(--compare-different-subtle);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
