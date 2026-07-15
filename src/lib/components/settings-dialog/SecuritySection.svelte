<script lang="ts">
  import type { Settings } from "$lib/api";

  interface Props {
    value: Settings["security"];
    biometryAvailable: boolean;
    quickUnlockMethod: string;
  }

  let {
    value = $bindable(),
    biometryAvailable,
    quickUnlockMethod,
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
      />
    </label>
    <label class="checkbox">
      <input type="checkbox" bind:checked={value.lock_on_sleep} />
      <span>Lock when the computer sleeps</span>
    </label>
  </div>
  <label class="checkbox">
    <input type="checkbox" bind:checked={value.touch_id} disabled={!biometryAvailable} />
    <span>
      Unlock with {quickUnlockMethod}{biometryAvailable ? "" : " (not available)"}
    </span>
  </label>
</fieldset>
