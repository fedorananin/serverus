<script lang="ts">
  // Approval for one AI agent action on a server whose access level is
  // "ask". Requests queue up; the oldest is shown first. Nothing gets focus:
  // the dialog pops up while the user may be typing, and a stray Enter or
  // space must neither approve the action nor reach the terminal behind.
  import { useAppModel } from "$lib/app/model.svelte";

  const agent = useAppModel().agent;
  const pending = $derived(agent.confirmations[0] ?? null);
  const waiting = $derived(agent.confirmations.length - 1);

  $effect(() => {
    if (pending && document.activeElement instanceof HTMLElement) document.activeElement.blur();
  });

  function onkeydown(event: KeyboardEvent) {
    if (!pending) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      void agent.decide(pending.requestId, "deny");
    }
  }
</script>

<svelte:window {onkeydown} />

{#if pending}
  <div class="backdrop" role="presentation">
    <div class="dialog" role="dialog" aria-label="AI agent request">
      <h2>🤖 The AI agent asks for permission</h2>
      <p>
        <strong>{pending.server}</strong> — {pending.action}:
      </p>
      <pre class="detail mono">{pending.detail}</pre>
      {#if waiting > 0}
        <p class="queue">{waiting} more request{waiting === 1 ? "" : "s"} waiting</p>
      {/if}
      <div class="actions">
        <button onclick={() => void agent.decide(pending.requestId, "deny")}>Deny</button>
        <button
          title="Allow every action on this server for 15 minutes without asking"
          onclick={() => void agent.decide(pending.requestId, "for_a_while")}
        >
          Allow for 15 min
        </button>
        <button class="primary" onclick={() => void agent.decide(pending.requestId, "once")}>
          Allow once
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--overlay-strong);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 310;
  }

  .dialog {
    width: 520px;
    max-width: calc(100vw - 40px);
    background: var(--bg-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    padding: 18px;
  }

  h2 {
    margin: 0 0 10px;
    font-size: 15px;
  }

  p {
    margin: 0 0 10px;
    color: var(--text-1);
  }

  .detail {
    margin: 0 0 12px;
    max-height: 240px;
    overflow: auto;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }

  .queue {
    font-size: 11px;
    color: var(--text-2);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
