<script lang="ts">
  import type { Settings } from "$lib/api";

  interface Props {
    value: Settings["transfers"];
  }

  let { value = $bindable() }: Props = $props();
</script>

<fieldset>
  <legend>Transfers</legend>
  <div class="row">
    <label>
      <span>Parallel files per server</span>
      <input type="number" min="1" max="16" bind:value={value.max_parallel_per_server} />
    </label>
    <label>
      <span>On conflict</span>
      <select bind:value={value.conflict_policy}>
        <option value="ask">Ask</option>
        <option value="overwrite">Overwrite</option>
        <option value="skip">Skip</option>
        <option value="rename">Rename</option>
      </select>
    </label>
  </div>
  <label class="checkbox">
    <input type="checkbox" bind:checked={value.preserve_mtime} />
    <span>Preserve modification times</span>
  </label>
  <label class="checkbox">
    <input type="checkbox" bind:checked={value.tar_acceleration} />
    <span>Accelerate folder transfers via tar stream when available</span>
  </label>
</fieldset>
