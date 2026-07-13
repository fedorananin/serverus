<script lang="ts">
  import type { Badge } from "$lib/api";

  interface Props {
    value: Badge | null;
    onchange: (badge: Badge | null) => void;
  }

  let { value, onchange }: Props = $props();

  // SPEC §3: ~10 preset colors + arbitrary hex.
  const PRESET_COLORS = [
    "#e5484d", "#f76b15", "#d29922", "#3fb950", "#0fbbb5",
    "#4a9eff", "#8250df", "#d84a91", "#8b949e", "#e6edf3",
  ];

  let emoji = $state(value?.kind === "emoji" ? value.value : "");
  let customHex = $state(value?.kind === "color" && !PRESET_COLORS.includes(value.value) ? value.value : "");

  function pickColor(color: string) {
    emoji = "";
    onchange({ kind: "color", value: color });
  }

  function pickEmoji(input: string) {
    // Keep only the first grapheme so pasting text doesn't break layout.
    const first = [...new Intl.Segmenter().segment(input)][0]?.segment ?? "";
    emoji = first;
    onchange(first ? { kind: "emoji", value: first } : null);
  }

  function pickCustomHex(hex: string) {
    customHex = hex;
    if (/^#[0-9a-fA-F]{6}$/.test(hex)) {
      emoji = "";
      onchange({ kind: "color", value: hex });
    }
  }
</script>

<div class="picker">
  <div class="colors">
    <button
      type="button"
      class="swatch none"
      class:active={!value}
      title="No badge"
      aria-label="No badge"
      onclick={() => {
        emoji = "";
        customHex = "";
        onchange(null);
      }}>✕</button
    >
    {#each PRESET_COLORS as color (color)}
      <button
        type="button"
        class="swatch"
        class:active={value?.kind === "color" && value.value === color}
        style:background={color}
        title={color}
        aria-label={color}
        onclick={() => pickColor(color)}
      ></button>
    {/each}
  </div>
  <div class="custom">
    <input
      type="text"
      placeholder="🐘 emoji"
      maxlength="8"
      value={emoji}
      oninput={(e) => pickEmoji(e.currentTarget.value)}
    />
    <input
      type="text"
      placeholder="#hex"
      maxlength="7"
      class="mono"
      value={customHex}
      oninput={(e) => pickCustomHex(e.currentTarget.value)}
    />
  </div>
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .colors {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid transparent;
    padding: 0;
    flex-shrink: 0;
  }

  .swatch.active {
    border-color: var(--text-0);
  }

  .swatch.none {
    background: var(--bg-2);
    color: var(--text-2);
    font-size: 10px;
    line-height: 1;
  }

  .custom {
    display: flex;
    gap: 8px;
  }

  .custom input {
    width: 90px;
  }
</style>
