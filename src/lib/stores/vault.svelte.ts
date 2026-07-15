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
  private accessEpoch = 0;
  private accessRevocationHandler: (() => void) | null = null;
  private contextRetirementHandler: (() => void) | null = null;

  /** Frontend-only authorization generation used to reject late UI results. */
  get accessGeneration(): number {
    return this.accessEpoch;
  }

  isAccessCurrent(generation: number): boolean {
    return this.data !== null && generation === this.accessEpoch;
  }

  get screen(): "loading" | "unlock" | "main" {
    if (!this.info) return "loading";
    return this.data ? "main" : "unlock";
  }

  async init() {
    this.info = await unwrap(commands.vaultGetInfo());
    await events.vaultLockedEvent.listen(() => {
      this.invalidateAccess();
      void this.refreshInfo();
    });
    // Auto-offer Touch ID on start (SPEC §5.1).
    if (this.info.exists && this.info.quick_unlock_ready) {
      await this.unlockQuick();
    }
  }

  async refreshInfo(epoch = this.accessEpoch) {
    const info = await unwrap(commands.vaultGetInfo());
    if (epoch === this.accessEpoch) this.info = info;
  }

  private async run(op: () => Promise<PublicVault>) {
    const epoch = this.accessEpoch;
    this.busy = true;
    this.error = null;
    try {
      const data = await op();
      if (epoch !== this.accessEpoch) return false;
      this.data = data;
      await this.refreshInfo(epoch);
      return true;
    } catch (e) {
      // A cancelled Touch ID prompt is not an error worth showing.
      if (
        epoch === this.accessEpoch &&
        !(isApiError(e) && e.code === "quick_unlock_cancelled")
      ) {
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
    this.invalidateAccess();
    await this.refreshInfo();
  }

  private invalidateAccess() {
    this.accessEpoch += 1;
    this.data = null;
    this.accessRevocationHandler?.();
  }

  /** Register app-shell cleanup for state authorized by one unlock epoch.
   *  Ordinary lock keeps sessions alive, so only access-bound UI is retired. */
  onAccessRevoked(handler: () => void): () => void {
    this.accessRevocationHandler = handler;
    return () => {
      if (this.accessRevocationHandler === handler) this.accessRevocationHandler = null;
    };
  }

  /** Register the app-shell hook that retires frontend state only after the
   *  backend has committed a vault-context switch. */
  onContextRetired(handler: () => void): () => void {
    this.contextRetirementHandler = handler;
    return () => {
      if (this.contextRetirementHandler === handler) this.contextRetirementHandler = null;
    };
  }

  /** Lock-screen action: point the app at a different vault file. An
   *  existing file shows its unlock form, a fresh path the create form. */
  async switchVault(path: string) {
    if (this.busy) return;
    this.busy = true;
    this.error = null;
    try {
      try {
        await unwrap(commands.vaultSwitchPath(path));
      } catch (e) {
        this.error = errorMessage(e);
        // Selection is already committed when runtime cleanup starts. A
        // cleanup failure leaves the runtime fail-closed until restart.
        if (!(isApiError(e) && e.code === "runtime_cleanup_failed")) return;
      }
      this.invalidateAccess();
      try {
        this.contextRetirementHandler?.();
      } finally {
        // A frontend listener cannot roll back a committed backend switch.
        await this.refreshInfo();
      }
    } finally {
      this.busy = false;
    }
  }

  // -- Connections & tree (M1). All mutations return the fresh PublicVault. --

  private async mutate(op: () => Promise<PublicVault>): Promise<void> {
    const epoch = this.accessEpoch;
    const data = await op();
    if (epoch === this.accessEpoch) this.data = data;
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
    const epoch = this.accessEpoch;
    const report = await unwrap(commands.vaultImportConfig(path));
    if (epoch === this.accessEpoch) this.data = report.vault;
    return report.connections;
  }

  removeKnownHost(host: string) {
    return this.mutate(() => unwrap(commands.knownHostRemove(host)));
  }
}

export const vault = new VaultStore();
