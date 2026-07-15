<script lang="ts">
  interface Props {
    remoteDir: string;
    localDir: string;
  }

  let { remoteDir = $bindable(), localDir = $bindable() }: Props = $props();
</script>

<div class="row">
  <label class="grow">
    <span>Remote start dir</span>
    <input
      type="text"
      aria-label="Remote start directory"
      bind:value={remoteDir}
      placeholder="/var/www"
      class="mono"
    />
  </label>
  <label class="grow">
    <span>Local start dir</span>
    <input
      type="text"
      aria-label="Local start directory"
      bind:value={localDir}
      placeholder="~/Projects"
      class="mono"
    />
  </label>
</div>
