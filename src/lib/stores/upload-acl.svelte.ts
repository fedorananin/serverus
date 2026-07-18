// S3 upload-ACL flow (SPEC §4.4): the pane's private/public/ask mode switch
// and the per-batch dialog shown in "ask" mode. Inert for non-S3 sessions —
// `mode` stays null and `ensure` always passes.

import type { S3UploadAcl } from "$lib/api";
import { commands, errorMessage, unwrap } from "$lib/api";
import { vault } from "$lib/stores/vault.svelte";

export type UploadAclChoice = "private" | "public_read" | null;

export class UploadAclController {
  private readonly sessionId: string;
  private readonly connectionId: string;
  private readonly s3: boolean;
  private readonly onerror: (message: string) => void;

  /** Pending "ask" dialog; null while closed. */
  ask = $state<{ count: number; resolve: (choice: UploadAclChoice) => void } | null>(null);

  /** Stored upload mode for the pane's mode switch; null hides it (non-S3). */
  readonly mode: S3UploadAcl | null = $derived.by(() =>
    this.s3 ? (vault.data?.connections[this.connectionId]?.s3?.upload_acl ?? "private") : null,
  );

  constructor(
    sessionId: string,
    connectionId: string,
    s3: boolean,
    onerror: (message: string) => void,
  ) {
    this.sessionId = sessionId;
    this.connectionId = connectionId;
    this.s3 = s3;
    this.onerror = onerror;
  }

  /** Persist a new stored upload mode. */
  async setMode(mode: S3UploadAcl) {
    try {
      const updated = await unwrap(commands.s3SetUploadAcl(this.sessionId, mode, true));
      if (updated) vault.data = updated;
    } catch (e) {
      this.onerror(errorMessage(e));
    }
  }

  /** In "ask" mode, resolve the batch ACL before enqueueing; false = cancel. */
  async ensure(count: number): Promise<boolean> {
    if (this.mode !== "ask") return true;
    const choice = await new Promise<UploadAclChoice>((resolve) => {
      this.ask = { count, resolve };
    });
    this.ask = null;
    if (!choice) return false;
    try {
      // Applies to this session only — the stored mode stays "ask".
      await unwrap(commands.s3SetUploadAcl(this.sessionId, choice, false));
      return true;
    } catch (e) {
      this.onerror(errorMessage(e));
      return false;
    }
  }
}
