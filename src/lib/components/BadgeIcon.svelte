<script lang="ts">
  import type { Badge } from "$lib/api";

  interface Props {
    badge: Badge | null | undefined;
    /** Fallback glyph when no badge is set (e.g. folder / server icons). */
    fallback?: string;
  }

  let { badge, fallback = "" }: Props = $props();
</script>

{#if badge?.kind === "emoji"}
  <span class="emoji">{badge.value}</span>
{:else if badge?.kind === "color"}
  <span class="dot" style:background={badge.value}></span>
{:else if fallback}
  <span class="emoji fallback">{fallback}</span>
{/if}

<style>
  .emoji {
    width: 16px;
    display: inline-flex;
    justify-content: center;
    font-size: 12px;
    flex-shrink: 0;
  }

  .fallback {
    opacity: 0.75;
  }

  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    box-shadow: 0 0 0 1px var(--badge-outline);
    display: inline-block;
    margin: 0 3.5px;
    flex-shrink: 0;
  }
</style>
