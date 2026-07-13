// Host key verification prompt (SPEC §4.1): one global dialog, connections
// wait for the user's verdict and retry on acceptance.

import { commands, unwrap } from "$lib/api";
import type { HostKeyPrompt } from "$lib/api";

interface PendingPrompt extends HostKeyPrompt {
  accepted: () => void;
  rejected: () => void;
}

class HostKeyStore {
  pending = $state<PendingPrompt | null>(null);

  ask(prompt: HostKeyPrompt, callbacks: { accepted: () => void; rejected: () => void }) {
    this.pending = { ...prompt, ...callbacks };
  }

  async accept() {
    const p = this.pending;
    if (!p) return;
    this.pending = null;
    await unwrap(commands.hostKeyAccept(p.host, p.port, p.key_line));
    p.accepted();
  }

  reject() {
    const p = this.pending;
    this.pending = null;
    p?.rejected();
  }
}

export const hostKey = new HostKeyStore();
