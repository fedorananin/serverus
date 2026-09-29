<script lang="ts">
  // What an AI agent may do on a connection (or on everything in a folder).
  // Null inherits from the enclosing folder; at the top level that is off.
  import type { AgentAccessLevel } from "$lib/api";

  let { value = $bindable() }: { value: AgentAccessLevel | null } = $props();

  const options: { value: AgentAccessLevel | "inherit"; label: string }[] = [
    { value: "inherit", label: "Inherit from folder (off at top level)" },
    { value: "off", label: "Off — hidden from agents" },
    { value: "read_only", label: "Read-only — list and read files" },
    { value: "ask", label: "Ask — confirm every change or command" },
    { value: "full", label: "Full — everything, never ask" },
  ];
</script>

<label>
  <span>AI agent access</span>
  <select
    aria-label="AI agent access"
    value={value ?? "inherit"}
    onchange={(event) => {
      const picked = event.currentTarget.value;
      value = picked === "inherit" ? null : (picked as AgentAccessLevel);
    }}
  >
    {#each options as option (option.value)}
      <option value={option.value}>{option.label}</option>
    {/each}
  </select>
</label>
