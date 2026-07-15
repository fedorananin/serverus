<script lang="ts">
  import type { Settings } from "$lib/api";

  interface Props {
    value: Settings["editor"];
  }

  let { value = $bindable() }: Props = $props();
</script>

<fieldset>
  <legend>Editor</legend>
  <label class="checkbox">
    <input type="checkbox" bind:checked={value.use_system_default} />
    <span>Open remote files with the system default app</span>
  </label>
  {#if !value.use_system_default}
    <label>
      <span>Application</span>
      <input type="text" bind:value={value.custom_app} placeholder="Visual Studio Code" />
    </label>
  {/if}
</fieldset>
