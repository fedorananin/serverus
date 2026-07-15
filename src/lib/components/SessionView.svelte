<script lang="ts">
  import type { Tab } from "$lib/stores/tabs.svelte";
  import { tabs } from "$lib/stores/tabs.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import TerminalPanel from "./TerminalPanel.svelte";
  import FilesView from "./FilesView.svelte";
  import TunnelsView from "./TunnelsView.svelte";

  interface Props {
    tab: Tab;
  }

  let { tab }: Props = $props();

  const connection = $derived(vault.data?.connections[tab.connectionId] ?? null);
  // FTP and S3 sessions have no shell or tunnels — Files is the only view.
  const filesOnly = $derived(connection?.protocol === "ftp" || connection?.protocol === "s3");
  const terminalDisabled = $derived(connection?.disable_terminal ?? false);
  // No tunnels configured → no Tunnels tab (add them via connection edit).
  const hasTunnels = $derived((connection?.tunnels.length ?? 0) > 0);

  // If the active view's tab disappeared (settings changed), fall back.
  $effect(() => {
    if (tab.view === "terminal" && (filesOnly || terminalDisabled)) tab.view = "files";
    if (tab.view === "tunnels" && (filesOnly || !hasTunnels)) tab.view = "files";
  });

  // Elapsed-seconds ticker while connecting: a slow connect (DNS, far-away
  // host, bastion chain) must look alive, not frozen.
  let elapsed = $state(0);
  $effect(() => {
    if (tab.state !== "connecting") return;
    elapsed = 0;
    const started = Date.now();
    const ticker = setInterval(() => {
      elapsed = Math.floor((Date.now() - started) / 1000);
    }, 1000);
    return () => clearInterval(ticker);
  });
</script>

<div class="session">
  <div class="viewbar">
    <div class="views">
      <button class:active={tab.view === "files"} onclick={() => (tab.view = "files")}>Files</button>
      {#if !filesOnly}
        {#if !terminalDisabled}
          <button class:active={tab.view === "terminal"} onclick={() => (tab.view = "terminal")}>
            Terminal
          </button>
        {/if}
        {#if hasTunnels}
          <button class:active={tab.view === "tunnels"} onclick={() => (tab.view = "tunnels")}>
            Tunnels
          </button>
        {/if}
      {/if}
    </div>
    <div
      class="status"
      data-testid="session-state"
      data-state={tab.state}
      role="status"
      aria-label="Connection {tab.state}"
    >
      <span class="dot" data-state={tab.state}></span>
      <span class="label">
        {#if tab.state === "connecting"}connecting…{/if}
        {#if tab.state === "connected"}{connection?.auth.username}@{connection?.host}{/if}
        {#if tab.state === "error"}error{/if}
        {#if tab.state === "disconnected"}disconnected{/if}
      </span>
    </div>
  </div>

  <div class="view-content">
    {#if tab.state === "error"}
      <div class="center">
        <p class="error">{tab.error}</p>
        <button onclick={() => void tabs.connect(tab.id)}>Retry</button>
      </div>
    {:else if tab.state === "connecting"}
      <div class="center">
        <div class="spinner" aria-hidden="true"></div>
        <p class="dim">Connecting to {connection?.host}…</p>
        {#if tab.connectMessage}
          <p class="stage">{tab.connectMessage}</p>
        {/if}
        {#if elapsed >= 5}
          <p class="stage">{elapsed}s — a slow network or a far-away host can take a while</p>
        {/if}
      </div>
    {:else if tab.sessionId}
      {#key tab.sessionId}
        <!-- All views stay mounted; switching hides them. Unmounting the
             terminal would close its shell channel and lose the session. -->
        {#if !filesOnly && !terminalDisabled}
          <div class="view-pane" style:display={tab.view === "terminal" ? "flex" : "none"}>
            <TerminalPanel sessionId={tab.sessionId} />
          </div>
        {/if}
        <div class="view-pane" style:display={tab.view === "files" ? "flex" : "none"}>
          <FilesView {tab} sessionId={tab.sessionId} />
        </div>
        {#if !filesOnly && hasTunnels}
          <div class="view-pane" style:display={tab.view === "tunnels" ? "flex" : "none"}>
            <TunnelsView sessionId={tab.sessionId} connectionId={tab.connectionId} />
          </div>
        {/if}
      {/key}
    {/if}
  </div>
</div>

<style>
  .session {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    flex: 1;
  }

  .viewbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 5px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-1);
  }

  .views {
    display: flex;
    gap: 2px;
  }

  .views button {
    background: transparent;
    border: 1px solid transparent;
    padding: 3px 12px;
    font-size: 12px;
    color: var(--text-1);
  }

  .views button.active {
    background: var(--bg-3);
    border-color: var(--border);
    color: var(--text-0);
  }

  .status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-1);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-2);
  }

  .dot[data-state="connected"] {
    background: var(--accent);
  }

  .dot[data-state="connecting"] {
    background: var(--warning);
  }

  .dot[data-state="error"] {
    background: var(--danger);
  }

  .view-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .view-pane {
    flex: 1;
    min-height: 0;
    flex-direction: column;
    position: relative;
  }

  .center {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }

  .dim {
    color: var(--text-2);
  }

  .stage {
    color: var(--text-2);
    font-size: 11px;
    margin: 0;
  }

  .spinner {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid var(--bg-3);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .error {
    color: var(--danger);
    max-width: 420px;
    text-align: center;
    user-select: text;
  }
</style>
