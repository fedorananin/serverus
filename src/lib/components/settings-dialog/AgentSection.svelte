<script lang="ts">
  // AI agent (MCP) access: the master switch, the "everything, no
  // questions" switch, and how to connect an agent such as Claude Code.
  import type { AgentSettings, AgentSetupInfo } from "$lib/api";
  import { useOptionalAppModel } from "$lib/app/model.svelte";

  let { value = $bindable() }: { value: AgentSettings } = $props();

  const api = useOptionalAppModel()?.api.agent;
  let setup = $state<AgentSetupInfo | null>(null);
  let copied = $state(false);
  void api
    ?.setupInfo()
    .then((info) => (setup = info))
    .catch(() => {});

  async function copyCommand() {
    if (!setup) return;
    await navigator.clipboard.writeText(setup.claude_code);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<fieldset>
  <legend>AI Agent</legend>
  <label class="checkbox">
    <input type="checkbox" bind:checked={value.enabled} disabled={setup?.supported === false} />
    <span>Let AI agents (MCP, e.g. Claude Code) work with my servers</span>
  </label>
  <label class="checkbox">
    <input type="checkbox" bind:checked={value.full_access} disabled={!value.enabled} />
    <span>Full access to every server — never ask</span>
  </label>
  <label>
    <span>Agent may add connections (with the passwords or keys you give it)</span>
    {#if value.full_access}
      <!-- Full access decides regardless of the stored choice, which stays
           for when full access is switched off again. -->
      <select aria-label="Agent may add connections" disabled>
        <option>Yes — included in full access</option>
      </select>
    {:else}
      <select
        aria-label="Agent may add connections"
        bind:value={value.create_connections}
        disabled={!value.enabled}
      >
        <option value="off">No</option>
        <option value="ask">Ask me for each one</option>
        <option value="allowed">Yes, without asking</option>
      </select>
    {/if}
  </label>
  {#if value.enabled && value.full_access}
    <p class="warning">
      The agent can see every server and run anything on it without confirmation. Per-server
      levels are ignored while this is on.
    </p>
  {:else}
    <p class="hint">
      Otherwise each connection or folder decides (Edit → AI agent access): off, read-only, ask
      before changes, or full. Folders pass their level down.
    </p>
  {/if}
  {#if value.enabled}
    <p class="hint">
      While an agent is connected and working with Serverus, the vault does not lock on idle or
      sleep — it locks again once the agent disconnects. The 🔒 button always locks.
    </p>
  {/if}
  {#if setup}
    {#if setup.problem}
      <p class="warning">{setup.problem}</p>
    {:else}
      <div class="setup">
        <span>Connect Claude Code (run once in a terminal):</span>
        <div class="command">
          <code class="mono">{setup.claude_code}</code>
          <button onclick={() => void copyCommand()}>{copied ? "Copied ✓" : "Copy"}</button>
        </div>
        {#if setup.command_warning}
          <p class="warning">{setup.command_warning}</p>
        {/if}
        <span class="hint">
          Other MCP clients: stdio server <code class="mono">{setup.command} {setup.args.join(" ")}</code>
        </span>
      </div>
    {/if}
  {/if}
</fieldset>

<style>
  p {
    margin: 0;
    font-size: 11px;
  }

  .hint {
    color: var(--text-2);
    font-size: 11px;
  }

  .warning {
    color: var(--warning);
  }

  .setup {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-1);
  }

  .command {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  code {
    flex: 1;
    min-width: 0;
    padding: 4px 6px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 11px;
    word-break: break-all;
    user-select: text;
  }

  .hint code {
    padding: 0 4px;
  }
</style>
