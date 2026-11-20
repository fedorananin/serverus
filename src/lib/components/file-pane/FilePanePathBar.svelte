<script lang="ts">
  import { pathCrumbs } from "$lib/path-segments";
  import type { PaneController } from "$lib/stores/pane.svelte";

  interface Props {
    pane: PaneController;
    title: string;
  }

  let { pane, title }: Props = $props();
  let pathInput = $state("");
  let editingPath = $state(false);
  let crumbsNode = $state<HTMLElement | null>(null);

  const crumbs = $derived(pathCrumbs(pane.path));
  const ancestors = $derived(crumbs.segments.slice(0, -1));
  const current = $derived(crumbs.segments.at(-1)?.label ?? pane.path);

  // A deep path keeps the folder you are in visible: scroll to the end
  // whenever the path changes (and when leaving the editor).
  $effect(() => {
    void pane.path;
    if (crumbsNode) crumbsNode.scrollLeft = crumbsNode.scrollWidth;
  });

  function startPathEdit() {
    pathInput = pane.path;
    editingPath = true;
  }

  function commitPath(event: Event) {
    event.preventDefault();
    editingPath = false;
    const target = pathInput.trim();
    if (target && target !== pane.path) void pane.navigate(target);
  }

  function open(path: string) {
    if (path !== pane.path) void pane.navigate(path);
  }

  /** A separator follows every folder crumb, except a root that already
   *  ends in one (`/`, `C:\`); a UNC share root (`\\nas\media`) gets one. */
  function needsSeparator(label: string) {
    return !label.endsWith("/") && !label.endsWith("\\");
  }

  function autofocus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<div class="pathbar">
  {#if editingPath}
    <form onsubmit={commitPath} class="path-form">
      <input
        type="text"
        aria-label="{title} path input"
        class="mono path-input"
        bind:value={pathInput}
        onblur={() => (editingPath = false)}
        onkeydown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            editingPath = false;
          }
        }}
        use:autofocus
      />
    </form>
  {:else}
    <nav class="crumbs mono" aria-label="{title} folders" bind:this={crumbsNode}>
      <!-- No whitespace between crumbs: the text must read as the path. -->
      {#each ancestors as segment (segment.path)}<button
          class="crumb"
          title="Open {segment.path}"
          onclick={() => open(segment.path)}>{segment.label}</button
        >{#if needsSeparator(segment.label)}<span class="sep" aria-hidden="true"
            >{crumbs.separator}</span
          >{/if}{/each}<button
        class="crumb current"
        title={pane.path}
        aria-label="{title} path"
        onclick={startPathEdit}>{current}</button
      >
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <span class="fill" onclick={startPathEdit}></span>
    </nav>
  {/if}
  <input type="text" class="filter" placeholder="Filter" bind:value={pane.filter} />
</div>

<style>
  .pathbar {
    display: flex;
    gap: 6px;
    padding: 5px 8px;
  }

  .crumbs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    overflow-x: auto;
    scrollbar-width: none;
    font-size: 11px;
    padding: 0 3px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-2);
    cursor: text;
  }

  .crumbs::-webkit-scrollbar {
    display: none;
  }

  .crumb {
    flex-shrink: 0;
    padding: 2px 3px;
    border: none;
    border-radius: 3px;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    white-space: nowrap;
    cursor: pointer;
  }

  .crumb:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }

  .crumb.current {
    color: var(--text-0);
    cursor: text;
  }

  .sep {
    flex-shrink: 0;
    color: var(--text-2);
  }

  .fill {
    flex: 1;
    align-self: stretch;
    min-width: 12px;
  }

  .path-form {
    flex: 1;
    display: flex;
    min-width: 0;
  }

  .path-input {
    width: 100%;
    font-size: 11px;
    padding: 3px 8px;
  }

  .filter {
    width: 84px;
    font-size: 11px;
    padding: 3px 7px;
  }
</style>
