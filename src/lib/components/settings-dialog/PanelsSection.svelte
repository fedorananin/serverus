<script lang="ts">
  import type { Settings } from "$lib/api";

  interface Props {
    value: Settings["panels"];
  }

  let { value = $bindable() }: Props = $props();
</script>

<fieldset>
  <legend>Panels</legend>
  <div class="row">
    <label class="checkbox">
      <input type="checkbox" bind:checked={value.show_hidden} />
      <span>Show hidden files</span>
    </label>
    <label>
      <span>Size format</span>
      <select bind:value={value.size_format}>
        <option value="kib">KiB (1024)</option>
        <option value="kb">KB (1000)</option>
      </select>
    </label>
  </div>
  <label>
    <span>Default local folder</span>
    <input type="text" bind:value={value.default_local_dir} placeholder="~" class="mono" />
  </label>
</fieldset>
