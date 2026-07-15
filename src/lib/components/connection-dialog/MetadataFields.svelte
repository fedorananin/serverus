<script lang="ts">
  import type { Badge } from "$lib/api";
  import BadgePicker from "../BadgePicker.svelte";

  interface Props {
    badge: Badge | null;
    notes: string;
  }

  let { badge = $bindable(), notes = $bindable() }: Props = $props();
</script>

<label>
  <span>Badge</span>
  <BadgePicker value={badge} onchange={(value) => (badge = value)} />
</label>

<label>
  <span>Notes</span>
  <textarea rows="2" bind:value={notes}></textarea>
</label>
