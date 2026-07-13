<script lang="ts">
  // Tunnels view (SPEC §4.2): saved forwards with start/stop and live
  // traffic counters.
  import { onMount } from "svelte";
  import type { TunnelStatus } from "$lib/api";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import { formatSize } from "$lib/format";

  interface Props {
    sessionId: string;
    connectionId: string;
  }

  let { sessionId, connectionId }: Props = $props();

  const connection = $derived(vault.data?.connections[connectionId] ?? null);
  let active = $state<TunnelStatus[]>([]);
  let error = $state<string | null>(null);
  let busyName = $state<string | null>(null);

  async function refresh() {
    try {
      active = await unwrap(commands.tunnelList(sessionId));
    } catch {
      // Session may be gone; the view unmounts shortly after.
    }
  }

  onMount(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 1000);
    return () => clearInterval(timer);
  });

  function activeFor(name: string, localPort: number): TunnelStatus | null {
    return active.find((t) => t.name === name && t.local_port === localPort) ?? null;
  }

  async function start(name: string, localPort: number, remoteHost: string, remotePort: number) {
    error = null;
    busyName = name;
    try {
      await unwrap(commands.tunnelStart(sessionId, name, localPort, remoteHost, remotePort));
      await refresh();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busyName = null;
    }
  }

  async function stop(tunnelId: string) {
    error = null;
    await unwrap(commands.tunnelStop(tunnelId));
    await refresh();
  }
</script>

<div class="tunnels">
  {#if (connection?.tunnels.length ?? 0) === 0}
    <div class="empty">
      <p class="dim">No tunnels configured for this connection.</p>
      <p class="dim small">Add them in the connection settings (right-click → Edit).</p>
    </div>
  {:else}
    <div class="list">
      {#each connection?.tunnels ?? [] as t (t.name + t.local_port)}
        {@const running = activeFor(t.name, t.local_port)}
        <div class="tunnel" class:running={running !== null}>
          <span class="dot" class:on={running !== null}></span>
          <div class="info">
            <div class="name">
              {t.name || "unnamed"}
              {#if t.autostart}<span class="auto">auto</span>{/if}
            </div>
            <div class="route mono">
              localhost:{t.local_port} → {t.remote_host}:{t.remote_port}
            </div>
          </div>
          {#if running}
            <div class="traffic mono">
              <span title="Uploaded">↑ {formatSize(running.bytes_up)}</span>
              <span title="Downloaded">↓ {formatSize(running.bytes_down)}</span>
              <span title="Open connections">{running.connections} conn</span>
            </div>
            <button onclick={() => void stop(running.id)}>Stop</button>
          {:else}
            <button
              class="primary"
              disabled={busyName === t.name}
              onclick={() => void start(t.name, t.local_port, t.remote_host, t.remote_port)}
            >
              {busyName === t.name ? "Starting…" : "Start"}
            </button>
          {/if}
        </div>
      {/each}
    </div>
    {#if error}
      <div class="error">{error}</div>
    {/if}
  {/if}
</div>

<style>
  .tunnels {
    padding: 16px;
    height: 100%;
    overflow-y: auto;
  }

  .empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
  }

  .dim {
    color: var(--text-2);
    margin: 0;
  }

  .small {
    font-size: 11px;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 620px;
  }

  .tunnel {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .tunnel.running {
    border-color: var(--accent-dim);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-2);
    flex-shrink: 0;
  }

  .dot.on {
    background: var(--accent);
  }

  .info {
    flex: 1;
    min-width: 0;
  }

  .name {
    font-weight: 500;
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .auto {
    font-size: 9px;
    color: var(--text-2);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0 4px;
  }

  .route {
    font-size: 11px;
    color: var(--text-1);
  }

  .traffic {
    display: flex;
    gap: 10px;
    font-size: 11px;
    color: var(--text-1);
  }

  .error {
    margin-top: 12px;
    color: var(--danger);
    font-size: 12px;
  }
</style>
