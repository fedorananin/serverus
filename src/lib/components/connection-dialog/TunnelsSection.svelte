<script lang="ts">
  import type { TunnelConfig } from "$lib/api";

  interface Props {
    tunnels: TunnelConfig[];
  }

  let { tunnels = $bindable() }: Props = $props();

  function addTunnel() {
    tunnels.push({
      name: "",
      kind: "local",
      local_port: 8080,
      remote_host: "127.0.0.1",
      remote_port: 8080,
      autostart: false,
    });
  }
</script>

<fieldset>
  <legend>Port tunnels</legend>
  {#each tunnels as tunnel, index (index)}
    <div class="tunnel mono">
      <input type="text" bind:value={tunnel.name} placeholder="name" class="t-name" />
      <input type="number" bind:value={tunnel.local_port} min="1" max="65535" title="Local port" />
      <span class="arrow">→</span>
      <input type="text" bind:value={tunnel.remote_host} placeholder="127.0.0.1" class="t-host" />
      <span>:</span>
      <input type="number" bind:value={tunnel.remote_port} min="1" max="65535" title="Remote port" />
      <label class="checkbox small" title="Start with connection">
        <input type="checkbox" bind:checked={tunnel.autostart} />
        <span>auto</span>
      </label>
      <button
        type="button"
        class="remove"
        onclick={() => tunnels.splice(index, 1)}
        aria-label="Remove tunnel">✕</button
      >
    </div>
  {/each}
  <button type="button" class="add" onclick={addTunnel}>+ Add tunnel</button>
</fieldset>
