<script lang="ts">
  import type { Snippet } from "svelte";
  import { modalLayer } from "$lib/stores/modal-stack";

  interface Props {
    title: string;
    width?: number;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  }

  let { title, width = 460, onclose, children, footer }: Props = $props();
</script>

<div class="backdrop" onmousedown={(e) => e.target === e.currentTarget && onclose()} role="presentation">
  <div
    class="modal"
    style:width="{width}px"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    use:modalLayer={{ onescape: onclose, trapFocus: true }}
  >
    <header>
      <span class="title">{title}</span>
      <button class="close" onclick={onclose} aria-label="Close">✕</button>
    </header>
    <div class="content">
      {@render children()}
    </div>
    {#if footer}
      <footer>
        {@render footer()}
      </footer>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 10vh;
    z-index: 100;
  }

  .modal {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    max-height: 78vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-lg);
  }

  /* Focused only as a fallback target on open — no ring around the dialog. */
  .modal:focus {
    outline: none;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    font-weight: 600;
  }

  .close {
    background: transparent;
    border: none;
    color: var(--text-1);
    padding: 2px 6px;
  }

  .content {
    padding: 16px;
    overflow-y: auto;
    user-select: none;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
</style>
