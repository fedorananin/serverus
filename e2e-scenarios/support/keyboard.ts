import { Key } from "webdriverio";

export const PRIMARY_SHORTCUT_KEYS = ["a", "t", "w", "1", "2", ","] as const;
export type PrimaryShortcutKey = (typeof PRIMARY_SHORTCUT_KEYS)[number];

const primaryShortcutKeys = new Set<string>(PRIMARY_SHORTCUT_KEYS);

export async function pressPrimaryShortcut(key: PrimaryShortcutKey): Promise<void> {
  if (!primaryShortcutKeys.has(key)) {
    throw new Error("The scenario requested an unsupported primary shortcut key.");
  }
  const primaryModifier = process.platform === "darwin" ? Key.Command : Key.Control;
  await browser
    .action("key")
    .down(primaryModifier)
    .down(key)
    .up(key)
    .up(primaryModifier)
    .perform();
}
