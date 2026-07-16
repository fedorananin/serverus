<script lang="ts">
  import { isMac } from "$lib/platform";

  interface Props {
    text: string;
    oncancel: () => void;
    onpaste: (text: string) => void;
    onrun: (text: string) => void;
  }

  let { text: initialText, oncancel, onpaste, onrun }: Props = $props();
  // The dialog owns an editable copy: it serves both the toolbar's compose
  // flow (starts empty) and the clipboard confirmation flow (starts filled),
  // and the user may fix up the command before sending either way.
  let text = $state(initialText);
  let textarea = $state<HTMLTextAreaElement>();
  const lines = $derived(text === "" ? 0 : text.split("\n").length);

  $effect(() => {
    textarea?.focus();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      oncancel();
      return;
    }
    // ⌘⏎ (Ctrl+⏎) sends from anywhere, including mid-edit in the textarea —
    // plain Enter must keep inserting newlines there.
    if (e.key === "Enter" && (isMac ? e.metaKey : e.ctrlKey)) {
      e.preventDefault();
      if (text) onrun(text);
    }
  }
</script>

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-label="Paste into terminal" tabindex="-1" onkeydown={onkeydown}>
    <p>
      Paste into the terminal
      {#if lines > 1}<span class="count">— {lines} lines</span>{/if}
    </p>
    <textarea
      class="mono"
      aria-label="Terminal paste text"
      bind:value={text}
      bind:this={textarea}
      spellcheck="false"
    ></textarea>
    <div class="actions">
      <span class="hint">{isMac ? "⌘⏎" : "Ctrl+⏎"} to paste and run</span>
      <button onclick={oncancel}>Cancel</button>
      <button disabled={!text} onclick={() => onpaste(text)}>Paste</button>
      <button class="primary" disabled={!text} onclick={() => onrun(text)}>Paste and run</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--overlay-soft);
  }
  .dialog {
    width: min(720px, calc(100vw - 80px));
    padding: 16px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  p {
    margin: 0 0 8px;
  }
  .count {
    color: var(--text-1);
  }
  textarea {
    width: 100%;
    height: 44vh;
    min-height: 200px;
    max-height: 60vh;
    resize: vertical;
    padding: 8px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 12px;
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 10px;
  }
  .hint {
    margin-right: auto;
    color: var(--text-2);
    font-size: 11px;
  }
</style>
