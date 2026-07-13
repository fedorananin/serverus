<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { tabs } from "$lib/stores/tabs.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import BadgeIcon from "$lib/components/BadgeIcon.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import SessionView from "$lib/components/SessionView.svelte";
  import HostKeyDialog from "$lib/components/HostKeyDialog.svelte";
  import TransferQueue from "$lib/components/TransferQueue.svelte";
  import ConflictDialog from "$lib/components/ConflictDialog.svelte";
  import Toasts, { showToast } from "$lib/components/Toasts.svelte";
  import DragGhost from "$lib/components/DragGhost.svelte";
  import { transfers } from "$lib/stores/transfers.svelte";
  import { commands, events, unwrap } from "$lib/api";

  let showSettings = $state(false);

  // -- Tab strip overflow (SPEC §5.1): tabs shrink evenly to a floor, then
  //    the strip scrolls horizontally with edge fades as the hint. --
  let tabStrip = $state<HTMLDivElement>();
  let fadeLeft = $state(false);
  let fadeRight = $state(false);

  function updateFades() {
    const el = tabStrip;
    if (!el) return;
    fadeLeft = el.scrollLeft > 2;
    fadeRight = el.scrollLeft + el.clientWidth < el.scrollWidth - 2;
  }

  // A vertical wheel gesture over the strip scrolls it horizontally
  // (horizontal trackpad swipes already work natively).
  function stripWheel(e: WheelEvent) {
    if (!tabStrip) return;
    if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) tabStrip.scrollLeft += e.deltaY;
  }

  // Keep the fades honest when tabs come and go, and keep the active tab
  // in view when it changes (e.g. ⌘1..9 or opening a new one).
  $effect(() => {
    void tabs.tabs.length;
    requestAnimationFrame(updateFades);
  });
  $effect(() => {
    const id = tabs.activeId;
    if (!id || !tabStrip) return;
    tabStrip
      .querySelector(`[data-tab-id="${CSS.escape(id)}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest", behavior: "smooth" });
  });

  $effect(() => {
    void transfers.init();
    // "Uploaded ✓" toast for remote-edit auto-uploads (SPEC §5.3).
    const unlisten = events.remoteEditUploadedEvent.listen((e) => {
      if (e.payload.error) {
        showToast(`Upload of ${e.payload.name} failed: ${e.payload.error}`, true);
      } else {
        showToast(`Uploaded ${e.payload.name} ✓`);
      }
    });
    return () => void unlisten.then((f) => f());
  });

  function connectionFor(tabConnectionId: string) {
    return vault.data?.connections[tabConnectionId] ?? null;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!e.metaKey) return;
    if (e.key === "w" && tabs.activeId) {
      e.preventDefault();
      tabs.close(tabs.activeId);
    } else if (e.key === "t" && tabs.active) {
      // New tab to the same server — a second independent session (SPEC §5.1).
      e.preventDefault();
      tabs.open(tabs.active.connectionId);
    } else if (e.key >= "1" && e.key <= "9") {
      e.preventDefault();
      tabs.activateIndex(Number(e.key) - 1);
    } else if (e.key === ",") {
      e.preventDefault();
      showSettings = true;
    }
  }

  // Throttled activity pings feed the auto-lock timer (SPEC §2.4).
  let lastActivityPing = 0;
  function reportActivity() {
    const now = Date.now();
    if (now - lastActivityPing > 15_000) {
      lastActivityPing = now;
      void unwrap(commands.vaultTouchActivity()).catch(() => {});
    }
  }
</script>

<svelte:window
  onkeydown={handleKeydown}
  onkeydowncapture={reportActivity}
  onmousedowncapture={reportActivity}
  onmousemove={reportActivity}
  onresize={updateFades}
/>

<div class="main">
  <Sidebar />

  <div class="content">
    <div class="tabbar">
      <div class="tabstrip-wrap" class:fade-left={fadeLeft} class:fade-right={fadeRight}>
        <div
          class="tabstrip"
          bind:this={tabStrip}
          onscroll={updateFades}
          onwheel={stripWheel}
          role="tablist"
          tabindex="-1"
        >
          {#each tabs.tabs as tab (tab.id)}
            {@const conn = connectionFor(tab.connectionId)}
            <div
              class="tab"
              class:active={tab.id === tabs.activeId}
              data-tab-id={tab.id}
              role="tab"
              tabindex="-1"
              aria-selected={tab.id === tabs.activeId}
              title={conn?.name ?? undefined}
              onclick={() => tabs.activate(tab.id)}
              onauxclick={(e) => e.button === 1 && tabs.close(tab.id)}
              onkeydown={(e) => e.key === "Enter" && tabs.activate(tab.id)}
            >
              <BadgeIcon badge={conn?.badge} fallback={conn?.protocol === "ftp" ? "📦" : "🖥"} />
              <span class="tab-name">{conn?.name ?? "?"}</span>
              <button
                class="tab-close"
                aria-label="Close tab"
                onclick={(e) => {
                  e.stopPropagation();
                  tabs.close(tab.id);
                }}>✕</button
              >
            </div>
          {/each}
        </div>
      </div>
      <button class="ghost" title="Settings (⌘,)" aria-label="Settings" onclick={() => (showSettings = true)}>⚙</button>
      <button class="ghost" title="Lock vault" aria-label="Lock vault" onclick={() => void vault.lock()}>🔒</button>
    </div>

    <div class="tab-content">
      {#each tabs.tabs as tab (tab.id)}
        <div class="tab-pane" style:display={tab.id === tabs.activeId ? "flex" : "none"}>
          <SessionView {tab} />
        </div>
      {/each}
      {#if !tabs.active}
        <div class="placeholder">
          <div class="logo mono">S<span class="accent">&gt;</span></div>
          <p class="dim">Double-click a connection to open a tab.</p>
        </div>
      {/if}
    </div>

    <TransferQueue />
  </div>
</div>

<HostKeyDialog />
<ConflictDialog />
<Toasts />
<DragGhost />

{#if showSettings && vault.data}
  <SettingsDialog onclose={() => (showSettings = false)} />
{/if}

<style>
  .main {
    height: 100%;
    display: flex;
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .tabbar {
    display: flex;
    align-items: center;
    gap: 2px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    padding: 4px 6px 0;
    min-height: 34px;
  }

  /* Overflow container: relative for the edge-fade hints. */
  .tabstrip-wrap {
    flex: 1;
    min-width: 0;
    position: relative;
  }

  .tabstrip {
    display: flex;
    gap: 2px;
    overflow-x: auto;
    scrollbar-width: none; /* the fades are the scroll hint */
    outline: none;
  }

  .tabstrip::-webkit-scrollbar {
    display: none;
  }

  /* Edge fades shown only when tabs are hidden past that edge. */
  .tabstrip-wrap::before,
  .tabstrip-wrap::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    width: 26px;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.15s;
    z-index: 2;
  }

  .tabstrip-wrap::before {
    left: 0;
    background: linear-gradient(90deg, var(--bg-1), transparent);
  }

  .tabstrip-wrap::after {
    right: 0;
    background: linear-gradient(270deg, var(--bg-1), transparent);
  }

  .tabstrip-wrap.fade-left::before {
    opacity: 1;
  }

  .tabstrip-wrap.fade-right::after {
    opacity: 1;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px 6px 10px;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--radius) var(--radius) 0 0;
    /* Safari-style: shrink evenly down to a floor, then the strip scrolls. */
    flex: 0 1 180px;
    min-width: 96px;
    max-width: 180px;
    cursor: default;
  }

  .tab:hover {
    background: var(--bg-2);
  }

  .tab.active {
    background: var(--bg-0);
    border-color: var(--border);
  }

  .tab-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }

  .tab-close {
    background: transparent;
    border: none;
    color: var(--text-2);
    font-size: 10px;
    padding: 1px 3px;
    border-radius: 3px;
    /* Narrow tabs: the ✕ only appears where it's actionable. */
    visibility: hidden;
  }

  .tab:hover .tab-close,
  .tab.active .tab-close {
    visibility: visible;
  }

  .tab-close:hover {
    color: var(--text-0);
    background: var(--bg-3);
  }

  .ghost {
    background: transparent;
    border: none;
    color: var(--text-1);
    font-size: 14px;
    padding: 3px 7px;
  }

  .ghost:hover {
    color: var(--text-0);
  }

  .tab-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .tab-pane {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
  }

  .logo {
    font-size: 34px;
    font-weight: 700;
    opacity: 0.5;
  }

  .accent {
    color: var(--accent);
  }

  .dim {
    color: var(--text-2);
  }
</style>
