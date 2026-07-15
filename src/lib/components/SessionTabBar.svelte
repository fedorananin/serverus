<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { tabs } from "$lib/stores/tabs.svelte";
  import BadgeIcon from "$lib/components/BadgeIcon.svelte";

  let { onOpenSettings }: { onOpenSettings: () => void } = $props();

  let tabStrip = $state<HTMLDivElement>();
  let fadeLeft = $state(false);
  let fadeRight = $state(false);

  function updateFades() {
    const element = tabStrip;
    if (!element) return;
    fadeLeft = element.scrollLeft > 2;
    fadeRight = element.scrollLeft + element.clientWidth < element.scrollWidth - 2;
  }

  function scrollHorizontally(event: WheelEvent) {
    if (!tabStrip) return;
    if (Math.abs(event.deltaY) > Math.abs(event.deltaX)) {
      tabStrip.scrollLeft += event.deltaY;
    }
  }

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

  function connectionFor(connectionId: string) {
    return vault.data?.connections[connectionId] ?? null;
  }
</script>

<svelte:window onresize={updateFades} />

<div class="tabbar">
  <div class="tabstrip-wrap" class:fade-left={fadeLeft} class:fade-right={fadeRight}>
    <div
      class="tabstrip"
      bind:this={tabStrip}
      onscroll={updateFades}
      onwheel={scrollHorizontally}
      role="tablist"
      tabindex="-1"
    >
      {#each tabs.tabs as tab (tab.id)}
        {@const connection = connectionFor(tab.connectionId)}
        <div
          class="tab"
          class:active={tab.id === tabs.activeId}
          data-tab-id={tab.id}
          role="tab"
          tabindex="-1"
          aria-selected={tab.id === tabs.activeId}
          title={connection?.name ?? undefined}
          onclick={() => tabs.activate(tab.id)}
          onauxclick={(event) => event.button === 1 && tabs.close(tab.id)}
          onkeydown={(event) => event.key === "Enter" && tabs.activate(tab.id)}
        >
          <BadgeIcon
            badge={connection?.badge}
            fallback={connection?.protocol === "ftp" ? "📦" : "🖥"}
          />
          <span class="tab-name">{connection?.name ?? "?"}</span>
          <button
            class="tab-close"
            aria-label="Close tab"
            onclick={(event) => {
              event.stopPropagation();
              tabs.close(tab.id);
            }}>✕</button
          >
        </div>
      {/each}
    </div>
  </div>
  <button class="ghost" title="Settings (⌘,)" aria-label="Settings" onclick={onOpenSettings}>⚙</button>
  <button class="ghost" title="Lock vault" aria-label="Lock vault" onclick={() => void vault.lock()}>🔒</button>
</div>

<style>
  .tabbar {
    display: flex;
    align-items: center;
    gap: 2px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    padding: 4px 6px 0;
    min-height: 34px;
  }

  .tabstrip-wrap {
    flex: 1;
    min-width: 0;
    position: relative;
  }

  .tabstrip {
    display: flex;
    gap: 2px;
    overflow-x: auto;
    scrollbar-width: none;
    outline: none;
  }

  .tabstrip::-webkit-scrollbar {
    display: none;
  }

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

  .tabstrip-wrap.fade-left::before,
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
</style>
