<script lang="ts">
  import type { Settings } from "$lib/api";

  interface Props {
    value: Settings["terminal"];
    isMac: boolean;
  }

  let { value = $bindable(), isMac }: Props = $props();
</script>

<fieldset>
  <legend>Terminal</legend>
  <div class="row">
    <label>
      <span>Font</span>
      <input type="text" bind:value={value.font_family} />
    </label>
    <label class="narrow">
      <span>Size</span>
      <input type="number" min="8" max="32" bind:value={value.font_size} />
    </label>
    <label class="narrow">
      <span>Scrollback</span>
      <input
        type="number"
        min="100"
        max="100000"
        step="100"
        bind:value={value.scrollback}
      />
    </label>
  </div>
  <label class="checkbox">
    <input type="checkbox" bind:checked={value.copy_on_select} />
    <span>
      Copy selection to clipboard automatically (otherwise use {isMac
        ? "⌘C"
        : "Ctrl+Shift+C"})
    </span>
  </label>
</fieldset>
