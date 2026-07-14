import { dnd } from "./dnd.svelte";
import { hostKey } from "./hostkey.svelte";
import { tabs } from "./tabs.svelte";
import { toasts } from "./toasts.svelte";
import { transfers } from "./transfers.svelte";
import { resetTerminalContext } from "$lib/terminals";

/** Drop every global frontend object whose identity belongs to one vault.
 *  This is deliberately state-only: vault_switch_path owns backend cleanup. */
export function resetVaultContext() {
  tabs.resetVaultContext();
  transfers.resetVaultContext();
  hostKey.resetVaultContext();
  dnd.resetVaultContext();
  toasts.resetVaultContext();
  resetTerminalContext();
}
