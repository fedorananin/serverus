<script lang="ts">
  import type { Settings } from "$lib/api";
  import { isMac, isWindows } from "$lib/platform";

  interface Props {
    value: Settings["editor"];
  }

  let { value = $bindable() }: Props = $props();

  // What opens those files when no fallback editor is set (editor.rs).
  const systemText = isMac
    ? "Empty: TextEdit, or your default text editor."
    : isWindows
      ? "Empty: Notepad."
      : "Empty: such files don't open.";
</script>

<fieldset>
  <legend>Editor</legend>
  <label class="checkbox">
    <input type="checkbox" bind:checked={value.use_system_default} />
    <span>Open files with the system default app</span>
  </label>
  {#if value.use_system_default}
    <label>
      <span>Editor for files without an app</span>
      <input type="text" bind:value={value.fallback_app} placeholder="Visual Studio Code" />
    </label>
    <span class="hint">
      Also opens files without an extension and files the system would run. {systemText}
    </span>
  {:else}
    <label>
      <span>Application</span>
      <input type="text" bind:value={value.custom_app} placeholder="Visual Studio Code" />
    </label>
  {/if}
</fieldset>
