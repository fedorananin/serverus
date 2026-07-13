<script lang="ts">
  import { dnd } from "$lib/stores/dnd.svelte";
</script>

{#if dnd.active}
  <div class="ghost" style:left="{dnd.x + 12}px" style:top="{dnd.y + 10}px">
    {dnd.label}
  </div>
{/if}

<style>
  .ghost {
    position: fixed;
    z-index: 400;
    pointer-events: none;
    background: var(--bg-2);
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    padding: 3px 10px;
    font-size: 11px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
