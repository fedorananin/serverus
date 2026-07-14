// Vault state: drives the Unlock ↔ Main screen switch and holds the
// decrypted (but secret-free) vault contents while unlocked.

import { commands, events, unwrap, errorMessage, isApiError } from "$lib/api";
import type {
  Badge,
  ConnectionInput,
  PublicVault,
  Settings,
  TreeNode,
  VaultInfo,
} from "$lib/api";
import { resetVaultContext } from "./reset-vault-context";
import { transfers } from "./transfers.svelte";

class VaultStore {
  info = $state<VaultInfo | null>(null);
  data = $state<PublicVault | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);
  /** Remounts all component-local state only when a vault boundary closes. */
  contextEpoch = $state(0);
  /** Even backend epoch accepted by commands and long-lived listeners. */
  runtimeEpoch = $state<number | null>(null);
  /** Old even epoch retained only to resume a failed teardown. */
  private retryEpoch: number | null = null;

  get screen(): "loading" | "unlock" | "main" {
    if (!this.info) return "loading";
    return this.data ? "main" : "unlock";
  }

  get contextOpen(): boolean {
    return this.runtimeEpoch !== null;
  }

  private applyInfo(info: VaultInfo) {
    this.info = info;
    if (info.context_epoch % 2 === 0) {
      this.runtimeEpoch = info.context_epoch;
      this.retryEpoch = null;
      transfers.setVaultContext(info.context_epoch);
    } else {
      this.runtimeEpoch = null;
      this.retryEpoch = info.context_epoch - 1;
      transfers.resetVaultContext();
    }
  }

  private closeFrontendContext() {
    this.runtimeEpoch = null;
    this.data = null;
    resetVaultContext();
    this.contextEpoch += 1;
  }

  async init() {
    const info = await unwrap(commands.vaultGetInfo());
    this.applyInfo(info);
    if (info.context_epoch % 2 !== 0) this.closeFrontendContext();
    await events.vaultLockedEvent.listen((event) => {
      if (event.payload.context_epoch !== this.runtimeEpoch) return;
      this.data = null;
      void this.refreshInfo();
    });
    // Auto-offer Touch ID on start (SPEC §5.1).
    if (this.contextOpen && info.exists && info.quick_unlock_ready) {
      await this.unlockQuick();
    }
  }

  async refreshInfo() {
    const wasOpen = this.runtimeEpoch !== null || this.data !== null;
    const info = await unwrap(commands.vaultGetInfo());
    this.applyInfo(info);
    if (info.context_epoch % 2 !== 0 && wasOpen) this.closeFrontendContext();
  }

  requireRuntimeEpoch(): number {
    const epoch = this.runtimeEpoch;
    if (epoch === null) throw new Error("Vault context is switching");
    return epoch;
  }

  private async run(op: (epoch: number) => Promise<PublicVault>) {
    if (this.busy || this.runtimeEpoch === null) return false;
    const epoch = this.runtimeEpoch;
    this.busy = true;
    this.error = null;
    try {
      const data = await op(epoch);
      if (this.runtimeEpoch === epoch) this.data = data;
      await this.refreshInfo();
      return true;
    } catch (e) {
      // A cancelled Touch ID prompt is not an error worth showing.
      if (!(isApiError(e) && e.code === "quick_unlock_cancelled")) {
        this.error = errorMessage(e);
      }
      return false;
    } finally {
      this.busy = false;
    }
  }

  create(password: string) {
    return this.run((epoch) => unwrap(commands.vaultCreate(password, epoch)));
  }

  unlockPassword(password: string) {
    return this.run((epoch) => unwrap(commands.vaultUnlockPassword(password, epoch)));
  }

  unlockQuick() {
    return this.run((epoch) => unwrap(commands.vaultUnlockQuick(epoch)));
  }

  async lock() {
    const epoch = this.requireRuntimeEpoch();
    await unwrap(commands.vaultLock(epoch));
    if (this.runtimeEpoch !== epoch) return;
    this.data = null;
    await this.refreshInfo();
  }

  /** Lock-screen action: point the app at a different vault file. An
   *  existing file shows its unlock form, a fresh path the create form. */
  async switchVault(path: string) {
    if (this.busy) return false;
    const epoch = this.runtimeEpoch ?? this.retryEpoch;
    if (epoch === null) {
      this.error = "Vault context is unavailable";
      return false;
    }

    this.busy = true;
    this.error = null;
    try {
      await unwrap(commands.vaultSwitchPath(path, epoch));
    } catch (e) {
      this.error = errorMessage(e);
      if (isApiError(e) && e.code === "vault_context_closed") {
        this.retryEpoch = epoch;
        if (this.runtimeEpoch !== null || this.data !== null) {
          this.closeFrontendContext();
        }
      }
      this.busy = false;
      return false;
    }

    // A normal lock intentionally preserves live sessions. A successful
    // switch is a hard boundary: discard A before reading or exposing B.
    this.retryEpoch = null;
    this.closeFrontendContext();
    try {
      await this.refreshInfo();
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.busy = false;
    }
    return true;
  }

  // -- Connections & tree (M1). All mutations return the fresh PublicVault. --

  private async mutate(
    epoch: number,
    op: () => Promise<PublicVault>,
  ): Promise<void> {
    const data = await op();
    if (this.runtimeEpoch === epoch) this.data = data;
  }

  upsertConnection(id: string | null, input: ConnectionInput, parentFolder: string | null) {
    const epoch = this.requireRuntimeEpoch();
    return this.mutate(epoch, () =>
      unwrap(commands.connectionUpsert(id, input, parentFolder, epoch)),
    );
  }

  duplicateConnection(id: string) {
    const epoch = this.requireRuntimeEpoch();
    return this.mutate(epoch, () => unwrap(commands.connectionDuplicate(id, epoch)));
  }

  deleteConnection(id: string) {
    const epoch = this.requireRuntimeEpoch();
    return this.mutate(epoch, () => unwrap(commands.connectionDelete(id, epoch)));
  }

  createFolder(name: string, parentFolder: string | null, badge: Badge | null) {
    const epoch = this.requireRuntimeEpoch();
    return this.mutate(epoch, () =>
      unwrap(commands.folderCreate(name, parentFolder, badge, epoch)),
    );
  }

  updateFolder(id: string, name: string, badge: Badge | null) {
    const epoch = this.requireRuntimeEpoch();
    return this.mutate(epoch, () => unwrap(commands.folderUpdate(id, name, badge, epoch)));
  }

  deleteFolder(id: string) {
    const epoch = this.requireRuntimeEpoch();
    return this.mutate(epoch, () => unwrap(commands.folderDelete(id, epoch)));
  }

  updateTree(tree: TreeNode[]) {
    const epoch = this.requireRuntimeEpoch();
    return this.mutate(epoch, () => unwrap(commands.treeUpdate(tree, epoch)));
  }

  updateSettings(settings: Settings) {
    const epoch = this.requireRuntimeEpoch();
    return this.mutate(epoch, () => unwrap(commands.settingsUpdate(settings, epoch)));
  }

  /** Import a config file; returns how many connections it brought in. */
  async importConfig(path: string): Promise<number> {
    const epoch = this.requireRuntimeEpoch();
    const report = await unwrap(commands.vaultImportConfig(path, epoch));
    if (this.runtimeEpoch === epoch) this.data = report.vault;
    return report.connections;
  }

  removeKnownHost(host: string) {
    const epoch = this.requireRuntimeEpoch();
    return this.mutate(epoch, () => unwrap(commands.knownHostRemove(host, epoch)));
  }
}

export const vault = new VaultStore();
