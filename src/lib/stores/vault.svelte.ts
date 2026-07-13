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

class VaultStore {
  info = $state<VaultInfo | null>(null);
  data = $state<PublicVault | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);

  get screen(): "loading" | "unlock" | "main" {
    if (!this.info) return "loading";
    return this.data ? "main" : "unlock";
  }

  async init() {
    this.info = await unwrap(commands.vaultGetInfo());
    await events.vaultLockedEvent.listen(() => {
      this.data = null;
      void this.refreshInfo();
    });
    // Auto-offer Touch ID on start (SPEC §5.1).
    if (this.info.exists && this.info.quick_unlock_ready) {
      await this.unlockQuick();
    }
  }

  async refreshInfo() {
    this.info = await unwrap(commands.vaultGetInfo());
  }

  private async run(op: () => Promise<PublicVault>) {
    this.busy = true;
    this.error = null;
    try {
      this.data = await op();
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
    return this.run(() => unwrap(commands.vaultCreate(password)));
  }

  unlockPassword(password: string) {
    return this.run(() => unwrap(commands.vaultUnlockPassword(password)));
  }

  unlockQuick() {
    return this.run(() => unwrap(commands.vaultUnlockQuick()));
  }

  async lock() {
    await unwrap(commands.vaultLock());
    this.data = null;
    await this.refreshInfo();
  }

  // -- Connections & tree (M1). All mutations return the fresh PublicVault. --

  private async mutate(op: () => Promise<PublicVault>): Promise<void> {
    this.data = await op();
  }

  upsertConnection(id: string | null, input: ConnectionInput, parentFolder: string | null) {
    return this.mutate(() => unwrap(commands.connectionUpsert(id, input, parentFolder)));
  }

  duplicateConnection(id: string) {
    return this.mutate(() => unwrap(commands.connectionDuplicate(id)));
  }

  deleteConnection(id: string) {
    return this.mutate(() => unwrap(commands.connectionDelete(id)));
  }

  createFolder(name: string, parentFolder: string | null, badge: Badge | null) {
    return this.mutate(() => unwrap(commands.folderCreate(name, parentFolder, badge)));
  }

  updateFolder(id: string, name: string, badge: Badge | null) {
    return this.mutate(() => unwrap(commands.folderUpdate(id, name, badge)));
  }

  deleteFolder(id: string) {
    return this.mutate(() => unwrap(commands.folderDelete(id)));
  }

  updateTree(tree: TreeNode[]) {
    return this.mutate(() => unwrap(commands.treeUpdate(tree)));
  }

  updateSettings(settings: Settings) {
    return this.mutate(() => unwrap(commands.settingsUpdate(settings)));
  }

  /** Import a config file; returns how many connections it brought in. */
  async importConfig(path: string): Promise<number> {
    const report = await unwrap(commands.vaultImportConfig(path));
    this.data = report.vault;
    return report.connections;
  }

  removeKnownHost(host: string) {
    return this.mutate(() => unwrap(commands.knownHostRemove(host)));
  }
}

export const vault = new VaultStore();
