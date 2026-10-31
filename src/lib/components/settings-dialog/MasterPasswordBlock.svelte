<script lang="ts">
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import RemovePasswordDialog from "./RemovePasswordDialog.svelte";

  let currentPassword = $state("");
  let newPassword = $state("");
  let confirmPassword = $state("");
  let status = $state<string | null>(null);
  let busy = $state(false);
  let removing = $state(false);

  const passwordless = $derived(vault.info?.passwordless === true);
  const canChange = $derived(
    !busy &&
      (passwordless || currentPassword !== "") &&
      newPassword !== "" &&
      newPassword === confirmPassword,
  );
  async function apply(next: string, done: string) {
    busy = true;
    status = null;
    try {
      await unwrap(commands.vaultChangePassword(passwordless ? "" : currentPassword, next));
      await vault.refreshInfo();
      status = done;
      currentPassword = newPassword = confirmPassword = "";
    } catch (e) {
      status = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if passwordless}
  <span class="hint">
    No master password: the vault opens without asking, and anyone who can read its file can read
    your saved passwords and keys.
  </span>
{/if}
<div class="pw-change">
  {#if !passwordless}
    <input type="password" placeholder="Current password" bind:value={currentPassword} />
  {/if}
  <input type="password" placeholder="New password" bind:value={newPassword} />
  <input type="password" placeholder="Repeat new" bind:value={confirmPassword} />
  {#if passwordless}
    <button
      type="button"
      disabled={!canChange}
      onclick={() => void apply(newPassword, "Master password set ✓")}
    >
      Set master password
    </button>
  {:else}
    <button
      type="button"
      disabled={!canChange}
      onclick={() => void apply(newPassword, "Master password changed ✓")}
    >
      Change master password
    </button>
    <button
      type="button"
      disabled={busy}
      onclick={() => {
        status = null;
        removing = true;
      }}
    >
      Remove master password…
    </button>
  {/if}
</div>
{#if status}
  <div class:error={!status.endsWith("✓")} class:ok={status.endsWith("✓")}>
    {status}
  </div>
{/if}
{#if removing}
  <RemovePasswordDialog
    onclose={() => (removing = false)}
    ondone={() => {
      status = "Master password removed ✓";
      currentPassword = newPassword = confirmPassword = "";
    }}
  />
{/if}
