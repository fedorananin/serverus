<script lang="ts">
  interface Props {
    knownHosts: [string, string][];
    onforget: (hostKey: string) => void;
  }

  let { knownHosts, onforget }: Props = $props();
</script>

{#if knownHosts.length > 0}
  <fieldset>
    <legend>Known hosts</legend>
    {#each knownHosts as [hostKey] (hostKey)}
      <div class="known-host">
        <span class="mono">{hostKey}</span>
        <button type="button" class="small" onclick={() => onforget(hostKey)}>Forget</button>
      </div>
    {/each}
  </fieldset>
{/if}
