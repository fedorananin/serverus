<script lang="ts">
  interface Props {
    text: string;
    oncancel: () => void;
    onpaste: () => void;
    onrun: () => void;
  }

  let { text, oncancel, onpaste, onrun }: Props = $props();
  const lines = $derived(text.split("\n").length);
  const preview = $derived(text.length > 400 ? text.slice(0, 400) + "…" : text);
</script>

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-label="Confirm terminal paste">
    <p>Paste {lines} lines into the terminal?</p>
    <pre class="mono">{preview}</pre>
    <div class="actions">
      <button onclick={oncancel}>Cancel</button>
      <button onclick={onpaste}>Paste</button>
      <button class="primary" onclick={onrun}>Paste and run</button>
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
    background: rgba(0, 0, 0, 0.5);
  }
  .dialog {
    width: 420px;
    padding: 16px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  p { margin: 0 0 8px; }
  pre {
    max-height: 160px;
    overflow: auto;
    padding: 8px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 11px;
    user-select: text;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 10px;
  }
</style>
