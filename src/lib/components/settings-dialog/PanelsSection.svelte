<script lang="ts">
  import type { Settings } from "$lib/api";

  interface Props {
    value: Settings["panels"];
  }

  let { value = $bindable() }: Props = $props();
</script>

<fieldset>
  <legend>Panels</legend>
  <label class="checkbox">
    <input type="checkbox" bind:checked={value.show_hidden} />
    <span>Show hidden files</span>
  </label>
  <label
    class="checkbox"
    title="Never show .DS_Store and Thumbs.db in the local pane, even with hidden files visible, and leave them out of folder comparison. Copies on the server stay visible so they can be deleted."
  >
    <input type="checkbox" bind:checked={value.hide_local_junk} />
    <span>Hide local junk files (.DS_Store, Thumbs.db)</span>
  </label>
  <div class="row">
    <label>
      <span>Size format</span>
      <select bind:value={value.size_format}>
        <option value="kib">KiB (1024)</option>
        <option value="kb">KB (1000)</option>
      </select>
    </label>
    <label>
      <span>Default local folder</span>
      <input type="text" bind:value={value.default_local_dir} placeholder="~" class="mono" />
    </label>
  </div>
</fieldset>
