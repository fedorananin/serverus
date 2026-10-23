// Host key verification prompts (SPEC §4.1): one dialog at a time,
// connections wait for the user's verdict and retry on acceptance. Prompts
// that arrive while another is on screen queue up behind it, and connections
// that present the same key for the same host share one prompt.

import { commands, isApiError, unwrap } from "$lib/api";
import type { HostKeyPrompt } from "$lib/api";

const ACCESS_REVOCATION_CODES = new Set([
  "vault_locked",
  "wrong_runtime_context",
  "runtime_context_switching",
]);

interface Waiter {
  /** Who waits (a tab id), so it can withdraw when it no longer does. */
  owner?: string;
  accepted: () => void;
  rejected: () => void;
}

interface QueuedPrompt {
  prompt: HostKeyPrompt;
  waiters: Waiter[];
}

function sameDecision(a: HostKeyPrompt, b: HostKeyPrompt): boolean {
  return (
    a.host === b.host &&
    a.port === b.port &&
    a.key_line === b.key_line &&
    a.runtime_context_id === b.runtime_context_id &&
    a.vault_access_epoch === b.vault_access_epoch
  );
}

class HostKeyStore {
  /** Prompts waiting for the user, oldest first; the dialog shows the head. */
  private queue = $state.raw<QueuedPrompt[]>([]);
  /** Accepted prompts whose key is still being saved. A connection that
   *  hits the same key meanwhile joins them instead of asking again. */
  private saving: QueuedPrompt[] = [];
  private contextGeneration = Symbol("runtime-context");

  get pending(): HostKeyPrompt | null {
    return this.queue[0]?.prompt ?? null;
  }

  /** Prompts waiting for the user, the one on screen included. */
  get waiting(): number {
    return this.queue.length;
  }

  ask(prompt: HostKeyPrompt, waiter: Waiter) {
    const shared = [...this.saving, ...this.queue].find((entry) =>
      sameDecision(entry.prompt, prompt),
    );
    if (shared) shared.waiters.push(waiter);
    else this.queue = [...this.queue, { prompt, waiters: [waiter] }];
  }

  async accept() {
    const entry = this.queue[0];
    if (!entry) return;
    const { prompt } = entry;
    const contextGeneration = this.contextGeneration;
    this.queue = this.queue.slice(1);
    this.saving.push(entry);
    try {
      await unwrap(
        commands.hostKeyAccept(
          prompt.host,
          prompt.port,
          prompt.key_line,
          prompt.runtime_context_id,
          prompt.vault_access_epoch,
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
      this.saving = this.saving.filter((other) => other !== entry);
      throw error;
    }
    if (this.contextGeneration !== contextGeneration) return;
    this.saving = this.saving.filter((other) => other !== entry);
    for (const waiter of entry.waiters) waiter.accepted();
  }

  reject() {
    const entry = this.queue[0];
    if (!entry) return;
    this.queue = this.queue.slice(1);
    for (const waiter of entry.waiters) waiter.rejected();
  }

  /** Stop waiting on behalf of `owner` (its tab closed or connects again).
   *  A prompt nobody waits for any more leaves the queue unanswered. */
  withdraw(owner: string) {
    for (const entry of [...this.saving, ...this.queue]) {
      entry.waiters = entry.waiters.filter((waiter) => waiter.owner !== owner);
    }
    if (this.queue.some((entry) => entry.waiters.length === 0)) {
      this.queue = this.queue.filter((entry) => entry.waiters.length > 0);
    }
  }

  /** Forget prompts whose vault authorization was revoked without treating
   *  lifecycle retirement as a user decision. */
  clearForAccessRevocation() {
    this.contextGeneration = Symbol("runtime-context");
    this.queue = [];
    this.saving = [];
  }

  /** Context retirement also revokes every decision issued by that context. */
  clearForContextRetirement() {
    this.clearForAccessRevocation();
  }
}

export const hostKey = new HostKeyStore();
