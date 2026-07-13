<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { SearchAddon } from "@xterm/addon-search";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import "@xterm/xterm/css/xterm.css";
  import { commands, unwrap } from "$lib/api";
  import { registerTerminal, unregisterTerminal } from "$lib/terminals";
  import { vault } from "$lib/stores/vault.svelte";

  interface Props {
    sessionId: string;
    /** Notifies the parent (terminal tab strip) that the shell ended. */
    onexit: () => void;
  }

  let { sessionId, onexit }: Props = $props();

  let container: HTMLDivElement;
  let termId: string | null = null;
  let exited = $state(false);
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchInput: HTMLInputElement | undefined = $state();

  let term: Terminal;
  let fit: FitAddon;
  let search: SearchAddon;
  let pendingPaste = $state<string | null>(null);

  async function confirmPaste() {
    if (termId && pendingPaste !== null) {
      await unwrap(commands.termWrite(termId, pendingPaste));
    }
    pendingPaste = null;
    term.focus();
  }

  onMount(() => {
    const settings = vault.data?.settings.terminal;
    term = new Terminal({
      fontFamily: `${settings?.font_family ?? "SF Mono"}, Menlo, monospace`,
      fontSize: settings?.font_size ?? 13,
      scrollback: settings?.scrollback ?? 10_000,
      cursorBlink: true,
      macOptionIsMeta: true,
      allowProposedApi: true,
      theme: {
        background: "#0d1117",
        foreground: "#e6edf3",
        cursor: "#3fb950",
        selectionBackground: "rgba(63, 185, 80, 0.3)",
      },
    });
    fit = new FitAddon();
    search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new WebLinksAddon());
    term.open(container);
    fit.fit();

    // SPEC §5.5: Cmd+C copies the selection, Cmd+V pastes (multiline pastes
    // are confirmed), Cmd+F searches. Copy-on-select is opt-in — always-on
    // silently clobbers whatever the user copied elsewhere.
    if (vault.data?.settings.terminal.copy_on_select) {
      term.onSelectionChange(() => {
        const sel = term.getSelection();
        if (sel) void navigator.clipboard.writeText(sel);
      });
    }
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      if (e.metaKey && e.key === "c" && term.hasSelection()) {
        void navigator.clipboard.writeText(term.getSelection());
        term.clearSelection();
        return false;
      }
      if (e.metaKey && e.key === "f") {
        searchOpen = true;
        queueMicrotask(() => searchInput?.focus());
        return false;
      }
      return true;
    });

    // Single paste path: intercept the DOM paste event at capture phase so
    // xterm's own handler never runs (running both pasted twice).
    const onPaste = (e: ClipboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const text = e.clipboardData?.getData("text/plain") ?? "";
      if (!text || !termId) return;
      if (text.includes("\n")) {
        pendingPaste = text;
      } else {
        void unwrap(commands.termWrite(termId, text));
      }
    };
    container.addEventListener("paste", onPaste, true);

    let disposed = false;
    void (async () => {
      const id = await unwrap(commands.termOpen(sessionId, term.cols, term.rows));
      if (disposed) {
        void unwrap(commands.termClose(id));
        return;
      }
      termId = id;
      registerTerminal(
        id,
        (data) => term.write(data),
        () => {
          exited = true;
          onexit();
        },
      );
      term.onData((data) => {
        void unwrap(commands.termWrite(id, data));
      });
      term.onResize(({ cols, rows }) => {
        void unwrap(commands.termResize(id, cols, rows));
      });
      term.focus();
    })();

    const observer = new ResizeObserver(() => {
      // The view can be hidden (display:none) while other session views are
      // active — fitting to a 0×0 box would collapse the terminal.
      if (container.clientWidth > 0 && container.clientHeight > 0) fit.fit();
    });
    observer.observe(container);

    return () => {
      disposed = true;
      observer.disconnect();
      container.removeEventListener("paste", onPaste, true);
      if (termId) {
        unregisterTerminal(termId);
        void unwrap(commands.termClose(termId)).catch(() => {});
      }
      term.dispose();
    };
  });

  function findNext(backwards = false) {
    if (!searchQuery) return;
    if (backwards) search.findPrevious(searchQuery);
    else search.findNext(searchQuery);
  }
</script>

<div class="terminal-wrap">
  {#if searchOpen}
    <div class="find-bar">
      <input
        type="text"
        placeholder="Find"
        bind:value={searchQuery}
        bind:this={searchInput}
        onkeydown={(e) => {
          if (e.key === "Enter") findNext(e.shiftKey);
          if (e.key === "Escape") {
            searchOpen = false;
            term.focus();
          }
        }}
      />
      <button onclick={() => findNext(true)} title="Previous">↑</button>
      <button onclick={() => findNext(false)} title="Next">↓</button>
      <button
        onclick={() => {
          searchOpen = false;
          term.focus();
        }}
        title="Close">✕</button
      >
    </div>
  {/if}
  <div class="terminal" bind:this={container}></div>
  {#if exited}
    <div class="exited">shell exited</div>
  {/if}
</div>

{#if pendingPaste !== null}
  {@const lines = pendingPaste.split("\n").length}
  <div class="paste-backdrop" role="presentation">
    <div class="paste-dialog">
      <p>Paste {lines} lines into the terminal?</p>
      <pre class="mono">{pendingPaste.length > 400 ? pendingPaste.slice(0, 400) + "…" : pendingPaste}</pre>
      <div class="paste-actions">
        <button onclick={() => (pendingPaste = null)}>Cancel</button>
        <button class="primary" onclick={() => void confirmPaste()}>Paste</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .terminal-wrap {
    position: relative;
    height: 100%;
    background: var(--bg-0);
  }

  .terminal {
    height: 100%;
    padding: 4px 0 0 6px;
  }

  .find-bar {
    position: absolute;
    top: 6px;
    right: 12px;
    z-index: 10;
    display: flex;
    gap: 4px;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    padding: 4px;
  }

  .find-bar input {
    width: 160px;
    font-size: 12px;
    padding: 3px 6px;
  }

  .find-bar button {
    padding: 2px 7px;
    font-size: 12px;
  }

  .exited {
    position: absolute;
    bottom: 10px;
    right: 12px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text-1);
    font-size: 11px;
    padding: 2px 8px;
  }

  .paste-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 150;
  }

  .paste-dialog {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 16px;
    width: 420px;
  }

  .paste-dialog p {
    margin: 0 0 8px;
  }

  .paste-dialog pre {
    max-height: 160px;
    overflow: auto;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px;
    font-size: 11px;
    user-select: text;
  }

  .paste-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 10px;
  }
</style>
