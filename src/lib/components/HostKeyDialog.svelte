<script lang="ts">
  import { hostKey } from "$lib/stores/hostkey.svelte";

  const p = $derived(hostKey.pending);
</script>

{#if p}
  <div class="backdrop" role="presentation">
    <div
      class="dialog"
      class:changed={p.changed}
      role="dialog"
      aria-label={p.changed ? "Host key changed" : "Unknown host"}
    >
      {#if p.changed}
        <h2>⚠️ Host key changed!</h2>
        <p class="warn">
          The key of <strong class="mono">{p.host}:{p.port}</strong> does not match the one stored
          earlier. Someone could be intercepting your connection (man-in-the-middle), or the server
          key was legitimately rotated. <strong>Do not accept unless you know why the key changed.</strong>
        </p>
      {:else}
        <h2>Unknown host</h2>
        <p>
          First connection to <strong class="mono">{p.host}:{p.port}</strong>. Verify the key
          fingerprint before trusting it.
        </p>
      {/if}

      <div class="fingerprint mono">
        <div>{p.algorithm}</div>
        <div>{p.fingerprint}</div>
      </div>

      <div class="actions">
        <button onclick={() => hostKey.reject()}>Cancel</button>
        <button class={p.changed ? "danger" : "primary"} onclick={() => void hostKey.accept()}>
          {p.changed ? "Accept new key anyway" : "Trust and connect"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 300;
  }

  .dialog {
    width: 440px;
    background: var(--bg-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    padding: 18px;
  }

  .dialog.changed {
    border-color: var(--danger);
    box-shadow: 0 0 0 1px var(--danger), 0 12px 40px rgba(229, 72, 77, 0.25);
  }

  h2 {
    margin: 0 0 10px;
    font-size: 15px;
  }

  .changed h2 {
    color: var(--danger);
  }

  p {
    margin: 0 0 12px;
    color: var(--text-1);
  }

  .warn {
    color: var(--text-0);
  }

  .fingerprint {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
    font-size: 12px;
    margin-bottom: 14px;
    user-select: text;
    word-break: break-all;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
