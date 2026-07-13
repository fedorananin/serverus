<script lang="ts">
  // Several terminals per connection tab — separate channels of one SSH
  // session (SPEC §5.5).
  import TerminalView from "./TerminalView.svelte";

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  let nextSlot = 1;
  let slots = $state<{ id: number }[]>([{ id: 0 }]);
  let activeSlot = $state(0);

  function addTerminal() {
    const id = nextSlot++;
    slots.push({ id });
    activeSlot = id;
  }

  function closeSlot(id: number) {
    const idx = slots.findIndex((s) => s.id === id);
    if (idx === -1) return;
    slots.splice(idx, 1);
    if (activeSlot === id) {
      activeSlot = slots[Math.min(idx, slots.length - 1)]?.id ?? -1;
    }
  }
</script>

<div class="panel">
  <div class="strip">
    {#each slots as slot, i (slot.id)}
      <button
        class="term-tab"
        class:active={slot.id === activeSlot}
        onclick={() => (activeSlot = slot.id)}
      >
        {i + 1}
        {#if slots.length > 1}
          <span
            class="x"
            role="button"
            tabindex="-1"
            onclick={(e) => {
              e.stopPropagation();
              closeSlot(slot.id);
            }}
            onkeydown={(e) => e.key === "Enter" && closeSlot(slot.id)}>✕</span
          >
        {/if}
      </button>
    {/each}
    <button class="add" onclick={addTerminal} title="New terminal">+</button>
  </div>
  <div class="terms">
    {#each slots as slot (slot.id)}
      <div class="term" style:display={slot.id === activeSlot ? "block" : "none"}>
        <TerminalView {sessionId} onexit={() => {}} />
      </div>
    {/each}
    {#if slots.length === 0}
      <div class="empty">
        <button onclick={addTerminal}>Open a terminal</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .strip {
    display: flex;
    gap: 3px;
    padding: 4px 8px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
  }

  .term-tab {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 2px 9px;
    font-size: 11px;
    background: transparent;
    border: 1px solid transparent;
  }

  .term-tab.active {
    background: var(--bg-3);
    border-color: var(--border);
  }

  .x {
    color: var(--text-2);
    font-size: 9px;
  }

  .x:hover {
    color: var(--text-0);
  }

  .add {
    padding: 2px 8px;
    font-size: 12px;
    background: transparent;
    border: none;
    color: var(--text-1);
  }

  .terms {
    flex: 1;
    min-height: 0;
    position: relative;
  }

  .term {
    height: 100%;
  }

  .empty {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
