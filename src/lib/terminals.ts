// Routes batched terminal-data events (base64 bytes) to the xterm instance
// that owns each term_id. One global listener for all terminals.

import { events } from "$lib/api";

type Sink = (data: Uint8Array) => void;
type ExitSink = () => void;

const sinks = new Map<string, Sink>();
const exitSinks = new Map<string, ExitSink>();
let listening = false;

function decode(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

async function ensureListener() {
  if (listening) return;
  listening = true;
  await events.terminalDataEvent.listen((e) => {
    sinks.get(e.payload.term_id)?.(decode(e.payload.data));
  });
  await events.terminalExitEvent.listen((e) => {
    exitSinks.get(e.payload.term_id)?.();
  });
}

export function registerTerminal(termId: string, sink: Sink, onExit: ExitSink) {
  void ensureListener();
  sinks.set(termId, sink);
  exitSinks.set(termId, onExit);
}

export function unregisterTerminal(termId: string) {
  sinks.delete(termId);
  exitSinks.delete(termId);
}
