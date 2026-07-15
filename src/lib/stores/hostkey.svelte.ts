// Host key verification prompt (SPEC §4.1): one global dialog, connections
// wait for the user's verdict and retry on acceptance.

import { commands, isApiError, unwrap } from "$lib/api";
import type { HostKeyPrompt } from "$lib/api";

const ACCESS_REVOCATION_CODES = new Set([
  "vault_locked",
  "wrong_runtime_context",
  "runtime_context_switching",
]);

interface PendingPrompt extends HostKeyPrompt {
  accepted: () => void;
  rejected: () => void;
}

class HostKeyStore {
  pending = $state<PendingPrompt | null>(null);
  private contextGeneration = Symbol("runtime-context");

  ask(prompt: HostKeyPrompt, callbacks: { accepted: () => void; rejected: () => void }) {
    this.pending = { ...prompt, ...callbacks };
  }

  async accept() {
    const p = this.pending;
    if (!p) return;
    const contextGeneration = this.contextGeneration;
    this.pending = null;
    try {
      await unwrap(
        commands.hostKeyAccept(
          p.host,
          p.port,
          p.key_line,
          p.runtime_context_id,
          p.vault_access_epoch,
        ),
      );
    } catch (error) {
      // The backend may revoke access before the frontend receives its lock
      // event. Only typed lifecycle failures retire this obsolete decision;
      // operational failures remain observable to the caller.
      if (isApiError(error) && ACCESS_REVOCATION_CODES.has(error.code)) {
        this.clearForAccessRevocation();
        return;
      }
      throw error;
    }
    if (this.contextGeneration === contextGeneration) p.accepted();
  }

  reject() {
    const p = this.pending;
    this.pending = null;
    p?.rejected();
  }

  /** Forget a prompt whose vault authorization was revoked without treating
   *  lifecycle retirement as a user decision. */
  clearForAccessRevocation() {
    this.contextGeneration = Symbol("runtime-context");
    this.pending = null;
  }

  /** Context retirement also revokes every decision issued by that context. */
  clearForContextRetirement() {
    this.clearForAccessRevocation();
  }
}

export const hostKey = new HostKeyStore();
