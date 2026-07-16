<script lang="ts">
  import { onMount } from "svelte";
  import { Channel } from "@tauri-apps/api/core";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { SearchAddon } from "@xterm/addon-search";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import "@xterm/xterm/css/xterm.css";
  import { commands, unwrap, type TerminalStreamEvent } from "$lib/api";
  import { isMac } from "$lib/platform";
  import { vault } from "$lib/stores/vault.svelte";
  import { needsPasteConfirmation } from "./terminal/paste";
  import { syncTerminalTheme, terminalThemeOptions } from "./terminal/terminal-theme";
  import TerminalPasteButton from "./TerminalPasteButton.svelte";
  import TerminalPasteConfirm from "./TerminalPasteConfirm.svelte";

  interface Props {
    sessionId: string;
    /** Notifies the parent (terminal tab strip) that the shell ended. */
    onexit: () => void;
  }

  let { sessionId, onexit }: Props = $props();

  let container: HTMLDivElement;
  let termId: string | null = null;
  let terminalReady = $state(false);
  let exited = $state(false);
  let searchOpen = $state(false);
  let searchQuery = $state("");
  let searchResult = $state<"idle" | "found" | "not-found">("idle");
  let searchInput: HTMLInputElement | undefined = $state();

  let term: Terminal;
  let fit: FitAddon;
  let search: SearchAddon;
  let pendingPaste = $state<string | null>(null);

  function decode(b64: string): Uint8Array {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }

  function queuePaste(text: string) {
    if (!text || !termId) return;
    if (needsPasteConfirmation(text)) pendingPaste = text;
    else term.paste(text);
  }

  function confirmPaste() {
    if (termId && pendingPaste !== null) {
      term.paste(pendingPaste);
    }
    pendingPaste = null;
    term.focus();
  }

  async function runPaste() {
    if (termId && pendingPaste !== null) {
      const command = pendingPaste.replace(/\r\n|\n/gu, "\r").replace(/\r*$/u, "\r");
      await unwrap(commands.termWrite(termId, command));
    }
    pendingPaste = null;
    term.focus();
  }

  function openSearch() {
    searchOpen = true;
    searchResult = "idle";
    queueMicrotask(() => searchInput?.focus());
  }

  function closeSearch() {
    searchOpen = false;
    searchResult = "idle";
    term.focus();
  }

  function findNext(backwards = false) {
    if (!searchQuery) {
      searchResult = "idle";
      return;
    }
    const found = backwards
      ? search.findPrevious(searchQuery)
      : search.findNext(searchQuery);
    searchResult = found ? "found" : "not-found";
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
      theme: terminalThemeOptions(),
    });
    fit = new FitAddon();
    search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new WebLinksAddon());
    term.open(container);
    fit.fit();
    const unsubscribeTheme = syncTerminalTheme(term);

    // SPEC §5.5: terminal copy, paste confirmation, and search shortcuts.
    if (vault.data?.settings.terminal.copy_on_select) {
      term.onSelectionChange(() => {
        const sel = term.getSelection();
        if (sel) void navigator.clipboard.writeText(sel);
      });
    }
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      // ⌘C/⌘F on macOS; Ctrl+Shift+C/F elsewhere — plain Ctrl+C must keep
      // reaching the shell as SIGINT.
      const combo = (key: string) =>
        isMac
          ? e.metaKey && e.key === key
          : e.ctrlKey && e.shiftKey && e.key.toLowerCase() === key;
      if (combo("c") && term.hasSelection()) {
        void navigator.clipboard.writeText(term.getSelection());
        term.clearSelection();
        return false;
      }
      if (combo("f")) {
        openSearch();
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
      queuePaste(text);
    };
    container.addEventListener("paste", onPaste, true);
    term.onWriteParsed(() => {
      if (searchOpen && searchQuery) findNext();
    });

    let disposed = false;
    void (async () => {
      const output = new Channel<TerminalStreamEvent>((event) => {
        if (disposed) return;
        if (event.kind === "data") term.write(decode(event.data));
        else {
          exited = true;
          onexit();
        }
      });
      const id = await unwrap(commands.termOpen(sessionId, term.cols, term.rows, output));
      if (disposed) {
        void unwrap(commands.termClose(id));
        return;
      }
      termId = id;
      term.onData((data) => {
        void unwrap(commands.termWrite(id, data));
      });
      term.onResize(({ cols, rows }) => {
        void unwrap(commands.termResize(id, cols, rows));
      });
      terminalReady = true;
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
      unsubscribeTheme();
      container.removeEventListener("paste", onPaste, true);
      if (termId) {
        void unwrap(commands.termClose(termId)).catch(() => {});
      }
      term.dispose();
    };
  });

</script>

<div class="terminal-wrap">
  {#if !terminalReady}<div class="opening" role="status">Opening terminal…</div>{/if}
  {#if terminalReady}<TerminalPasteButton onpaste={queuePaste} onfind={openSearch} />{/if}
  {#if searchOpen}
    <div class="find-bar">
      <input
        type="text"
        placeholder="Find"
        aria-label="Terminal find text"
        value={searchQuery}
        bind:this={searchInput}
        oninput={(event) => {
          searchQuery = event.currentTarget.value;
          findNext();
        }}
        onkeydown={(e) => {
          if (e.key === "Enter") findNext(e.shiftKey);
          if (e.key === "Escape") closeSearch();
        }}
      />
      <span class="find-result" role="status">
        {searchResult === "found" ? "Match found" : searchResult === "not-found" ? "No matches" : ""}
      </span>
      <button onclick={() => findNext(true)} title="Previous">↑</button>
      <button onclick={() => findNext(false)} title="Next">↓</button>
      <button onclick={closeSearch} title="Close">✕</button>
    </div>
  {/if}
  <div class="terminal" data-terminal-state={terminalReady ? "ready" : "opening"} bind:this={container}></div>
  {#if exited}
    <div class="exited">shell exited</div>
  {/if}
</div>

{#if pendingPaste !== null}
  <TerminalPasteConfirm
    text={pendingPaste}
    oncancel={() => (pendingPaste = null)}
    onpaste={confirmPaste}
    onrun={() => void runPaste()}
  />
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
  .opening {
    position: absolute;
    inset: 8px auto auto 10px;
    z-index: 1;
    color: var(--text-2);
    font-size: 12px;
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
  .find-result {
    align-self: center;
    min-width: 72px;
    color: var(--text-1);
    font-size: 11px;
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
</style>
