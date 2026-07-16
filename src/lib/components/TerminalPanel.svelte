<script lang="ts">
  // Several terminals per connection tab — separate channels of one SSH
  // session (SPEC §5.5).
  import TerminalView from "./TerminalView.svelte";
  import TerminalPasteButton from "./TerminalPasteButton.svelte";

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  let nextSlot = 1;
  let slots = $state<{ id: number }[]>([{ id: 0 }]);
  let activeSlot = $state(0);
  let views = $state<Record<number, TerminalView | undefined>>({});

  function addTerminal() {
    const id = nextSlot++;
    slots.push({ id });
    activeSlot = id;
  }

  /** Focus the visible terminal — used when this session's tab becomes active. */
  export function focusActive() {
    views[activeSlot]?.focusTerminal();
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
      <div
        class="term-tab"
        class:active={slot.id === activeSlot}
      >
        <button
          class="term-select"
          aria-label={`Terminal ${i + 1}`}
          aria-pressed={slot.id === activeSlot}
          onclick={() => {
            activeSlot = slot.id;
            views[slot.id]?.focusTerminal();
          }}
        >
          <span aria-hidden="true">{i + 1}</span>
        </button>
        {#if slots.length > 1}
          <button
            class="x"
            aria-label={`Close terminal ${i + 1}`}
            onclick={(e) => {
              e.stopPropagation();
              closeSlot(slot.id);
            }}>✕</button
          >
        {/if}
      </div>
    {/each}
    <button class="add" onclick={addTerminal} title="New terminal" aria-label="New terminal">+</button>
    {#if slots.length > 0}
      <div class="strip-actions">
        <TerminalPasteButton
          onpaste={() => views[activeSlot]?.openPasteDialog()}
          onfind={() => views[activeSlot]?.openSearch()}
        />
      </div>
    {/if}
  </div>
  <div class="terms">
    {#each slots as slot (slot.id)}
      <div class="term" style:display={slot.id === activeSlot ? "block" : "none"}>
        <TerminalView bind:this={views[slot.id]} {sessionId} onexit={() => {}} />
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
    align-items: center;
    gap: 3px;
    min-height: 28px;
    padding: 4px 8px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
  }

  .strip-actions {
    margin-left: auto;
  }

  .term-tab {
    display: flex;
    align-items: center;
    background: transparent;
    border: 1px solid transparent;
  }

  .term-tab.active {
    background: var(--bg-3);
    border-color: var(--border);
  }

  .term-select,
  .x {
    background: transparent;
    border: none;
    color: inherit;
  }

  .term-select {
    padding: 2px 9px;
    font-size: 11px;
  }

  /* When a close button follows, hand the right-side spacing over to it. */
  .term-tab:has(.x) .term-select {
    padding-right: 4px;
  }

  .x {
    color: var(--text-2);
    font-size: 9px;
    padding: 2px 7px 2px 2px;
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
