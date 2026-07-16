<script lang="ts">
  interface Props {
    /** Runs the search over the terminal buffer; returns whether a match exists. */
    find: (query: string, backwards: boolean) => boolean;
    onclose: () => void;
  }

  let { find, onclose }: Props = $props();

  let query = $state("");
  let result = $state<"idle" | "found" | "not-found">("idle");
  let input = $state<HTMLInputElement>();

  export function focusInput() {
    input?.focus();
  }

  /** Re-run the current search when new terminal output arrives. */
  export function refresh() {
    if (query) run();
  }

  function run(backwards = false) {
    if (!query) {
      result = "idle";
      return;
    }
    result = find(query, backwards) ? "found" : "not-found";
  }
</script>

<div class="find-bar">
  <input
    type="text"
    placeholder="Find"
    aria-label="Terminal find text"
    value={query}
    bind:this={input}
    oninput={(event) => {
      query = event.currentTarget.value;
      run();
    }}
    onkeydown={(e) => {
      if (e.key === "Enter") run(e.shiftKey);
      if (e.key === "Escape") onclose();
    }}
  />
  <span class="find-result" role="status">
    {result === "found" ? "Match found" : result === "not-found" ? "No matches" : ""}
  </span>
  <button onclick={() => run(true)} title="Previous">↑</button>
  <button onclick={() => run(false)} title="Next">↓</button>
  <button onclick={onclose} title="Close">✕</button>
</div>

<style>
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
</style>
