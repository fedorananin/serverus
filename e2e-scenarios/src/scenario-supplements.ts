import {
  ALL_PLATFORMS,
  type AcceptanceId,
  type ScenarioPlatform,
} from "./scenarios";

export interface ManualNativeAcceptanceSupplement {
  id: string;
  title: string;
  acceptanceId: AcceptanceId;
  automatedOwnerId: string;
  platforms: readonly ScenarioPlatform[];
  inputFidelity: "manual-native";
  manualAction: string;
  reason: string;
}

export const MANUAL_NATIVE_SUPPLEMENTS = [
  {
    id: "platform-shortcuts-arrow-transfer-native",
    title: "Transfer files with primary-modifier Left and Right shortcuts",
    acceptanceId: "AC-017",
    automatedOwnerId: "platform-shortcuts",
    platforms: ALL_PLATFORMS,
    inputFidelity: "manual-native",
    manualAction:
      "Select a local file and press Command/Control+Right, then select a remote file and press Command/Control+Left; verify both transfers complete with identical bytes.",
    reason:
      "tauri-plugin-wdio-webdriver 1.2.0 sends Arrow keys without retaining active modifier flags, so WebDriver cannot prove either native chord.",
  },
  {
    id: "platform-context-menu-native",
    title: "Open file actions with a native right-button click",
    acceptanceId: "AC-017",
    automatedOwnerId: "platform-shortcuts",
    platforms: ["darwin", "linux"],
    inputFidelity: "manual-native",
    manualAction:
      "Right-click a visible local and remote file row; verify the actions menu opens at the pointer and its enabled action works.",
    reason:
      "The embedded WebKit driver does not reliably deliver the native right-button path on macOS or Linux; Windows covers it automatically through WebView2.",
  },
  {
    id: "remote-edit-native-editor",
    title: "Edit a remote file in an installed native editor",
    acceptanceId: "AC-009",
    automatedOwnerId: "remote-edit-safety",
    platforms: ALL_PLATFORMS,
    inputFidelity: "manual-native",
    manualAction:
      "Configure an installed editor, open a remote file, save a unique change, and verify both the visible upload result and remote bytes.",
    reason:
      "The automated owner uses a deterministic external editor process; WebDriver cannot control arbitrary native editor windows or OS launchers.",
  },
] as const satisfies readonly ManualNativeAcceptanceSupplement[];

interface AutomatedSupplementOwner {
  id: string;
  acceptanceIds: readonly AcceptanceId[];
  platforms: readonly ScenarioPlatform[];
}

export function validateManualNativeSupplements(
  catalog: readonly ManualNativeAcceptanceSupplement[],
  automatedOwners: readonly AutomatedSupplementOwner[],
  reservedIds: readonly string[],
): string[] {
  const errors: string[] = [];
  const ids = new Set<string>();
  const reserved = new Set(reservedIds);

  for (const supplement of catalog) {
    if (reserved.has(supplement.id)) {
      errors.push(`${supplement.id}: conflicts with an acceptance owner id`);
    }
    if (ids.has(supplement.id)) {
      errors.push(`${supplement.id}: duplicate manual-native supplement id`);
    }
    ids.add(supplement.id);
    if (supplement.title.trim().length === 0) {
      errors.push(`${supplement.id}: title must not be empty`);
    }
    if (supplement.platforms.length === 0) {
      errors.push(`${supplement.id}: platforms must not be empty`);
    }
    if (new Set(supplement.platforms).size !== supplement.platforms.length) {
      errors.push(`${supplement.id}: platforms must not contain duplicates`);
    }

    const owner = automatedOwners.find(({ id }) => id === supplement.automatedOwnerId);
    if (!owner) {
      errors.push(`${supplement.id}: automated owner ${supplement.automatedOwnerId} does not exist`);
    } else {
      if (!owner.acceptanceIds.includes(supplement.acceptanceId)) {
        errors.push(
          `${supplement.id}: ${supplement.acceptanceId} is not owned by automated scenario ${owner.id}`,
        );
      }
      for (const platform of new Set(supplement.platforms)) {
        if (!owner.platforms.includes(platform)) {
          errors.push(`${supplement.id}: platform ${platform} is not automated by ${owner.id}`);
        }
      }
    }
    if (supplement.manualAction.trim().length === 0) {
      errors.push(`${supplement.id}: manualAction must not be empty`);
    }
    if (supplement.reason.trim().length === 0) {
      errors.push(`${supplement.id}: reason must not be empty`);
    }
  }

  return errors;
}
