<script lang="ts">
  import type { RemoteEntry, SizeFormat } from "$lib/api";
  import { isMod } from "$lib/platform";
  import { dnd } from "$lib/stores/dnd.svelte";
  import type { PaneController } from "$lib/stores/pane.svelte";
  import FilePaneRow from "./FilePaneRow.svelte";
  import { FilePaneMarquee } from "./file-pane-marquee.svelte";
  import { FilePaneNativeDrag } from "./file-pane-native-drag";
  import { FILE_ROW_HEIGHT, type FilePaneDialog } from "./types";

  interface Props {
    pane: PaneController;
    title: string;
    sizeFormat: SizeFormat;
    ontransfer: (entries: RemoteEntry[]) => void;
    onopenfile?: (entry: RemoteEntry) => void;
    onmenu: (event: MouseEvent | KeyboardEvent, entry: RemoteEntry | null) => void;
    ondialog: (dialog: FilePaneDialog) => void;
  }

  let { pane, title, sizeFormat, ontransfer, onopenfile, onmenu, ondialog }: Props = $props();
  const comparisonId = $props.id();
  let scroller: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewportHeight = $state(400);

  const total = $derived(pane.visible.length);
  const first = $derived(Math.max(0, Math.floor(scrollTop / FILE_ROW_HEIGHT) - 10));
  const last = $derived(
    Math.min(total, Math.ceil((scrollTop + viewportHeight) / FILE_ROW_HEIGHT) + 10),
  );
  const windowEntries = $derived(pane.visible.slice(first, last));
  const marquee = new FilePaneMarquee(
    () => pane,
    () => scroller,
    (value) => (scrollTop = value),
  );
  const nativeDrag = new FilePaneNativeDrag(() =>
    pane.selectedEntries.map((entry) => entry.path),
  );

  function rowDoubleClick(entry: RemoteEntry) {
    if (entry.is_dir) {
      void pane.open(entry);
    } else if (pane.side === "remote") {
      onopenfile?.(entry);
    }
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "F10" && event.shiftKey) {
      onmenu(event, pane.selectedEntries[0] ?? null);
    } else if (event.key === "F2" || event.key === "Enter") {
      const selection = pane.selectedEntries;
      if (selection.length === 1) {
        event.preventDefault();
        ondialog({ kind: "rename", entry: selection[0] });
      }
    } else if (
      isMod(event) &&
      event.key === (pane.side === "local" ? "ArrowRight" : "ArrowLeft") &&
      pane.selectedEntries.length > 0
    ) {
      event.preventDefault();
      event.stopPropagation();
      ontransfer(pane.selectedEntries);
    } else if (event.key === "Backspace" && !isMod(event)) {
      event.preventDefault();
      void pane.up();
    } else if (event.key === "a" && isMod(event)) {
      event.preventDefault();
      pane.selectAll();
    } else if (event.key === "Delete" || (event.key === "Backspace" && isMod(event))) {
      const selection = pane.selectedEntries;
      if (selection.length > 0) {
        event.preventDefault();
        ondialog({ kind: "delete", entries: selection });
      }
    }
  }

  function rowPointerDown(event: PointerEvent, entry: RemoteEntry) {
    if (event.button !== 0 || isMod(event) || event.shiftKey) return;
    if (marquee.isRowWhitespace(event)) return;
    if (!pane.selected.has(entry.name)) {
      pane.selected = new Set([entry.name]);
      pane.anchor = entry.name;
    }
    if (pane.side === "local") {
      nativeDrag.arm(event);
      return;
    }
    const entries = pane.selectedEntries;
    const label = entries.length === 1 ? entries[0].name : `${entries.length} items`;
    dnd.begin(event, { kind: "files", side: "remote" }, label, () => {
      if (dnd.paneTarget === "local") ontransfer(entries);
    });
  }
</script>

<div
  class="rows"
  bind:this={scroller}
  bind:clientHeight={viewportHeight}
  onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
  role="listbox"
  aria-label="{title} files"
  tabindex="0"
  onkeydown={keydown}
  onpointerdown={(event) => marquee.pointerDown(event)}
  oncontextmenu={(event) => onmenu(event, null)}
>
  {#if pane.error}
    <div class="pane-error">{pane.error}</div>
  {:else if pane.loading}
    <div class="pane-empty">Loading…</div>
  {:else if total === 0}
    <div class="pane-empty">Empty</div>
  {:else}
    <div style:height="{total * FILE_ROW_HEIGHT}px" class="spacer">
      <div style:transform="translateY({first * FILE_ROW_HEIGHT}px)">
        {#each windowEntries as entry, index (entry.path)}
          <FilePaneRow
            {entry}
            selected={pane.selected.has(entry.name)}
            s3={pane.s3}
            aclStatus={pane.acl[entry.path]}
            comparisonStatus={pane.comparisonStatuses?.get(entry.name)}
            comparisonDescriptionId={`${comparisonId}-comparison-${first + index}`}
            {sizeFormat}
            onclick={(event) => {
              scroller?.focus();
              if (!marquee.justEnded) pane.click(entry, event);
            }}
            ondoubleclick={() => rowDoubleClick(entry)}
            oncontextmenu={(event) => onmenu(event, entry)}
            onpointerdown={(event) => rowPointerDown(event, entry)}
          />
        {/each}
      </div>
    </div>
  {/if}
  {#if marquee.rect}
    <div
      class="marquee"
      style:left="{marquee.rect.x0}px"
      style:top="{marquee.rect.y0}px"
      style:width="{marquee.rect.x1 - marquee.rect.x0}px"
      style:height="{marquee.rect.y1 - marquee.rect.y0}px"
    ></div>
  {/if}
</div>

<style>
  .rows {
    flex: 1;
    overflow-y: auto;
    outline: none;
    position: relative;
    user-select: none;
    -webkit-user-select: none;
  }

  .spacer {
    position: relative;
    overflow: hidden;
  }

  .marquee {
    position: absolute;
    z-index: 5;
    border: 1px solid var(--accent);
    background: var(--accent-subtle);
    opacity: 0.6;
    pointer-events: none;
  }

  .pane-empty,
  .pane-error {
    padding: 20px;
    text-align: center;
    color: var(--text-2);
    font-size: 12px;
  }

  .pane-error {
    color: var(--danger);
    user-select: text;
  }
</style>
