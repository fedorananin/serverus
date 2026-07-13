<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";

  let password = $state("");
  let confirm = $state("");
  let passwordInput: HTMLInputElement | undefined = $state();

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
  }

  const creating = $derived(vault.info !== null && !vault.info.exists);
  const mismatch = $derived(creating && confirm.length > 0 && password !== confirm);
  const canSubmit = $derived(
    password.length > 0 && !vault.busy && (!creating || password === confirm),
  );

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    const ok = creating
      ? await vault.create(password)
      : await vault.unlockPassword(password);
    if (ok) {
      password = "";
      confirm = "";
    } else {
      passwordInput?.select();
    }
  }
</script>

<div class="unlock">
  <div class="card">
    <div class="logo mono">S<span class="accent">&gt;</span><span class="cursor"></span></div>
    <h1>{creating ? "Create your vault" : "Serverus"}</h1>

    {#if creating}
      <p class="hint">
        Pick a master password. It encrypts everything and is never stored —
        <strong>there is no way to recover it if forgotten.</strong>
      </p>
    {/if}

    <form onsubmit={submit}>
      <input
        type="password"
        placeholder="Master password"
        bind:value={password}
        bind:this={passwordInput}
        disabled={vault.busy}
        use:focusOnMount
      />
      {#if creating}
        <input
          type="password"
          placeholder="Repeat master password"
          bind:value={confirm}
          disabled={vault.busy}
        />
        {#if mismatch}
          <div class="error">Passwords do not match</div>
        {/if}
      {/if}

      {#if vault.error}
        <div class="error">{vault.error}</div>
      {/if}

      <button class="primary" type="submit" disabled={!canSubmit}>
        {#if vault.busy}
          {creating ? "Creating…" : "Unlocking…"}
        {:else}
          {creating ? "Create vault" : "Unlock"}
        {/if}
      </button>

      {#if !creating && vault.info?.quick_unlock_ready}
        <button
          type="button"
          class="touch-id"
          onclick={() => vault.unlockQuick()}
          disabled={vault.busy}
        >
          Use Touch ID
        </button>
      {/if}
    </form>

    <div class="path mono" title={vault.info?.path}>{vault.info?.path}</div>
  </div>
</div>

<style>
  .unlock {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .card {
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    text-align: center;
  }

  .logo {
    font-size: 40px;
    font-weight: 700;
    color: var(--text-0);
  }

  .logo .accent {
    color: var(--accent);
  }

  .cursor {
    display: inline-block;
    width: 0.45em;
    height: 0.72em;
    margin-left: 0.12em;
    background: var(--accent);
    opacity: 0.9;
    animation: blink 1.2s steps(1) infinite;
  }

  @keyframes blink {
    50% {
      opacity: 0;
    }
  }

  h1 {
    font-size: 16px;
    font-weight: 600;
    margin: 0;
  }

  .hint {
    color: var(--text-1);
    font-size: 12px;
    margin: 0;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  input {
    text-align: center;
  }

  .error {
    color: var(--danger);
    font-size: 12px;
  }

  .touch-id {
    background: transparent;
    border-color: transparent;
    color: var(--accent);
  }

  .path {
    margin-top: 16px;
    color: var(--text-2);
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
