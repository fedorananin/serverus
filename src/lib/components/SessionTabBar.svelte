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

  // Pointer-based tab reorder — HTML5 DnD does not work inside the Tauri
  // webview (same reason as dnd.svelte.ts). A drag starts only after the
  // pointer moves past a threshold, so plain clicks still just activate.
  const DRAG_THRESHOLD = 5;
  let draggingId = $state<string | null>(null);
  let pressed: { id: string; x: number; y: number } | null = null;

  function onTabPointerDown(event: PointerEvent, id: string) {
    if (event.button !== 0) return;
    if (event.target instanceof Element && event.target.closest(".tab-close")) return;
    pressed = { id, x: event.clientX, y: event.clientY };
    window.addEventListener("pointermove", onDragMove);
    window.addEventListener("pointerup", onDragEnd);
  }

  function onDragMove(event: PointerEvent) {
    if (!pressed) return;
    if (!draggingId) {
      const dist = Math.hypot(event.clientX - pressed.x, event.clientY - pressed.y);
      if (dist < DRAG_THRESHOLD) return;
      draggingId = pressed.id;
      tabs.activate(pressed.id);
    }
    autoscroll(event.clientX);
    // Insertion index = how many other tabs have their midpoint left of the pointer.
    const others = [...(tabStrip?.querySelectorAll<HTMLElement>("[data-tab-id]") ?? [])].filter(
      (el) => el.dataset.tabId !== draggingId,
    );
    let index = 0;
    for (const el of others) {
      const rect = el.getBoundingClientRect();
      if (event.clientX > rect.left + rect.width / 2) index += 1;
    }
    tabs.move(draggingId, index);
  }

  /** Nudge the strip while dragging near its edges so far-away slots are reachable. */
  function autoscroll(pointerX: number) {
    const element = tabStrip;
    if (!element) return;
    const rect = element.getBoundingClientRect();
    if (pointerX < rect.left + 24) element.scrollLeft -= 12;
    else if (pointerX > rect.right - 24) element.scrollLeft += 12;
  }

  function onDragEnd() {
    pressed = null;
    draggingId = null;
    window.removeEventListener("pointermove", onDragMove);
    window.removeEventListener("pointerup", onDragEnd);
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
          class:dragging={tab.id === draggingId}
          data-tab-id={tab.id}
          role="tab"
          tabindex="-1"
          aria-selected={tab.id === tabs.activeId}
          title={connection?.name ?? undefined}
          onpointerdown={(event) => onTabPointerDown(event, tab.id)}
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
    user-select: none;
    -webkit-user-select: none;
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
    gap: 4px;
    padding: 5px 3px 6px 7px;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--radius) var(--radius) 0 0;
    flex: 0 1 150px;
    min-width: 84px;
    max-width: 150px;
    cursor: default;
  }

  .tab:hover {
    background: var(--bg-2);
  }

  .tab.active {
    background: var(--bg-0);
    border-color: var(--border);
  }

  .tab.dragging {
    opacity: 0.7;
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
