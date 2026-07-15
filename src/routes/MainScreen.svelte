<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { tabs } from "$lib/stores/tabs.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import SettingsDialog from "$lib/components/SettingsDialog.svelte";
  import SessionView from "$lib/components/SessionView.svelte";
  import SessionTabBar from "$lib/components/SessionTabBar.svelte";
  import HostKeyDialog from "$lib/components/HostKeyDialog.svelte";
  import TransferQueue from "$lib/components/TransferQueue.svelte";
  import ConflictDialog from "$lib/components/ConflictDialog.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import DragGhost from "$lib/components/DragGhost.svelte";
  import { useAppModel } from "$lib/app/model.svelte";
  import { isMod } from "$lib/platform";

  const appModel = useAppModel();
  const transfers = appModel.transfers;
  let showSettings = $state(false);

  $effect(() => {
    if (vault.screen === "main") void transfers.init();
  });

  function handleKeydown(event: KeyboardEvent) {
    if (vault.screen !== "main") return;
    if (!isMod(event)) return;
    if (event.key === "w" && tabs.activeId) {
      event.preventDefault();
      tabs.close(tabs.activeId);
    } else if (event.key === "t" && tabs.active) {
      event.preventDefault();
      tabs.open(tabs.active.connectionId);
    } else if (event.key >= "1" && event.key <= "9") {
      event.preventDefault();
      tabs.activateIndex(Number(event.key) - 1);
    } else if (event.key === ",") {
      event.preventDefault();
      showSettings = true;
    }
  }

  // Throttled activity pings feed the auto-lock timer (SPEC §2.4).
  let lastActivityPing = 0;
  function reportActivity() {
    if (vault.screen !== "main") return;
    const now = Date.now();
    if (now - lastActivityPing > 15_000) {
      lastActivityPing = now;
      void appModel.api.vault.touchActivity().catch(() => {});
    }
  }
</script>

<svelte:window
  onkeydown={handleKeydown}
  onkeydowncapture={reportActivity}
  onmousedowncapture={reportActivity}
  onmousemove={reportActivity}
/>

<div
  class="screen"
  data-testid="main-screen"
  inert={vault.screen !== "main"}
  aria-hidden={vault.screen !== "main"}
>
  <div class="main">
    <Sidebar />

    <div class="content">
      <SessionTabBar onOpenSettings={() => (showSettings = true)} />

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
</div>

<style>
  .screen {
    height: 100%;
  }

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
