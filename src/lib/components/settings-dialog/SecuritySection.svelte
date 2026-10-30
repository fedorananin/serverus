<script lang="ts">
  import type { Settings } from "$lib/api";

  interface Props {
    value: Settings["security"];
    biometryAvailable: boolean;
    quickUnlockMethod: string;
    /** Nothing to lock or unlock: these settings wait for a master password. */
    passwordless: boolean;
  }

  let {
    value = $bindable(),
    biometryAvailable,
    quickUnlockMethod,
    passwordless,
  }: Props = $props();
</script>

<fieldset>
  <legend>Security</legend>
  <div class="row">
    <label>
      <span>Auto-lock after (minutes, 0 = never)</span>
      <input
        type="number"
        min="0"
        max="1440"
        aria-label="Auto-lock after (minutes, 0 = never)"
        bind:value={value.auto_lock_minutes}
        disabled={passwordless}
      />
    </label>
    <label class="checkbox">
      <input type="checkbox" bind:checked={value.lock_on_sleep} disabled={passwordless} />
      <span>Lock when the computer sleeps</span>
    </label>
  </div>
  <label class="checkbox">
    <input
      type="checkbox"
      bind:checked={value.touch_id}
      disabled={!biometryAvailable || passwordless}
    />
    <span>
      Unlock with {quickUnlockMethod}{biometryAvailable ? "" : " (not available)"}
    </span>
  </label>
  {#if passwordless}
    <span class="hint">
      The vault has no master password, so it never locks. Set one in Vault below to use these.
    </span>
  {/if}
</fieldset>
