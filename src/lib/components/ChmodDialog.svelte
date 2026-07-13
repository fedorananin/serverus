<script lang="ts">
  // Permissions dialog (SPEC §5.4): rwx grid ⇄ synced octal field, with
  // recursive apply for directories.
  import type { RemoteEntry } from "$lib/api";
  import Modal from "./Modal.svelte";

  interface Props {
    entry: RemoteEntry;
    /** Called with the mode and the recursive scope (null = only this). */
    onapply: (mode: number, recursive: "files" | "dirs" | "both" | null) => void;
    onclose: () => void;
  }

  let { entry, onapply, onclose }: Props = $props();

  let mode = $state(entry.permissions ?? 0o644);
  let octal = $state(((entry.permissions ?? 0o644) & 0o777).toString(8).padStart(3, "0"));
  let recursive = $state(false);
  let recursiveScope = $state<"files" | "dirs" | "both">("both");

  const ROLES = ["Owner", "Group", "Others"];
  const PERMS = ["r", "w", "x"];

  function bit(role: number, perm: number): number {
    return 0o400 >> (role * 3 + perm);
  }

  function toggle(role: number, perm: number) {
    mode = mode ^ bit(role, perm);
    octal = (mode & 0o777).toString(8).padStart(3, "0");
  }

  function setOctal(value: string) {
    octal = value;
    if (/^[0-7]{3,4}$/.test(value)) {
      mode = parseInt(value, 8);
    }
  }
</script>

<Modal title={`Permissions — ${entry.name}`} width={340} {onclose}>
  <div class="grid mono">
    <span></span>
    {#each PERMS as p (p)}<span class="head">{p}</span>{/each}
    {#each ROLES as role, r (role)}
      <span class="role">{role}</span>
      {#each PERMS as p, i (p)}
        <input
          type="checkbox"
          checked={(mode & bit(r, i)) !== 0}
          onchange={() => toggle(r, i)}
        />
      {/each}
    {/each}
  </div>

  <label class="octal">
    <span>Octal</span>
    <input
      type="text"
      class="mono"
      maxlength="4"
      value={octal}
      oninput={(e) => setOctal(e.currentTarget.value)}
    />
  </label>

  {#if entry.is_dir}
    <label class="rec">
      <input type="checkbox" bind:checked={recursive} />
      <span>Apply recursively to</span>
      <select bind:value={recursiveScope} disabled={!recursive}>
        <option value="both">files and dirs</option>
        <option value="files">files only</option>
        <option value="dirs">dirs only</option>
      </select>
    </label>
  {/if}

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button
      class="primary"
      onclick={() => {
        // Apply BEFORE closing: callers read their dialog state inside
        // onapply, and onclose clears it.
        onapply(mode & 0o7777, entry.is_dir && recursive ? recursiveScope : null);
        onclose();
      }}>Apply</button
    >
  {/snippet}
</Modal>

<style>
  .grid {
    display: grid;
    grid-template-columns: 70px repeat(3, 34px);
    gap: 6px 2px;
    align-items: center;
    margin-bottom: 14px;
  }

  .head {
    text-align: center;
    color: var(--text-1);
  }

  .role {
    color: var(--text-1);
    font-size: 12px;
  }

  .grid input {
    justify-self: center;
  }

  .octal {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .octal input {
    width: 64px;
    text-align: center;
  }

  .octal span {
    font-size: 12px;
    color: var(--text-1);
  }

  .rec {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
    font-size: 12px;
  }
</style>
