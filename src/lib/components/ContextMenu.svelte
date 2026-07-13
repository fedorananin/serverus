<script lang="ts" module>
  export interface MenuItem {
    label: string;
    danger?: boolean;
    disabled?: boolean;
    separator?: boolean;
    action?: () => void;
  }
</script>

<script lang="ts">
  interface Props {
    x: number;
    y: number;
    items: MenuItem[];
    onclose: () => void;
  }

  let { x, y, items, onclose }: Props = $props();

  let menu: HTMLDivElement | undefined = $state();

  // Keep the menu inside the window.
  const pos = $derived.by(() => {
    const w = menu?.offsetWidth ?? 180;
    const h = menu?.offsetHeight ?? items.length * 26;
    return {
      x: Math.min(x, window.innerWidth - w - 8),
      y: Math.min(y, window.innerHeight - h - 8),
    };
  });

  function pick(item: MenuItem) {
    if (item.disabled || item.separator) return;
    onclose();
    item.action?.();
  }
</script>

<svelte:window
  onmousedown={(e) => {
    if (menu && !menu.contains(e.target as Node)) onclose();
  }}
  onkeydown={(e) => e.key === "Escape" && onclose()}
  onblur={onclose}
/>

<div class="menu" bind:this={menu} style:left="{pos.x}px" style:top="{pos.y}px" role="menu" tabindex="-1">
  {#each items as item, i (i)}
    {#if item.separator}
      <div class="separator"></div>
    {:else}
      <button
        class="item"
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={() => pick(item)}
        role="menuitem"
      >
        {item.label}
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 200;
    min-width: 170px;
    background: var(--bg-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    padding: 4px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
  }

  .item {
    background: transparent;
    border: none;
    text-align: left;
    padding: 5px 10px;
    border-radius: 4px;
    width: 100%;
  }

  .item:hover:not(:disabled) {
    background: var(--bg-3);
  }

  .item.danger {
    color: var(--danger);
  }

  .separator {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
</style>
