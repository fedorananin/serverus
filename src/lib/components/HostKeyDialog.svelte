<script lang="ts">
  import { hostKey } from "$lib/stores/hostkey.svelte";
  import { modalLayer } from "$lib/stores/modal-stack";

  /** Prompts appear unasked and queue behind each other: a click meant for
   *  the previous one (or a double click) must not answer a key unseen. */
  const ARM_DELAY_MS = 500;

  const p = $derived(hostKey.pending);
  let armed = $state(false);

  $effect(() => {
    if (!p) return;
    armed = false;
    const timer = setTimeout(() => (armed = true), ARM_DELAY_MS);
    return () => clearTimeout(timer);
  });
</script>

{#if p}
  <div class="backdrop" role="presentation">
    <div
      class="dialog"
      class:changed={p.changed}
      role="dialog"
      aria-modal="true"
      aria-label={p.changed ? "Host key changed" : "Unknown host"}
      use:modalLayer
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
        {#if hostKey.waiting > 1}
          <span class="more">{hostKey.waiting - 1} more waiting</span>
        {/if}
        <button disabled={!armed} onclick={() => hostKey.reject()}>Cancel</button>
        <button
          class={p.changed ? "danger" : "primary"}
          disabled={!armed}
          onclick={() => void hostKey.accept()}
        >
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
    background: var(--overlay-strong);
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
    box-shadow: 0 0 0 1px var(--danger), var(--shadow-danger);
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
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .more {
    margin-right: auto;
    color: var(--text-2);
    font-size: 11px;
  }
</style>
