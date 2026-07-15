import type { ChildProcessWithoutNullStreams } from "node:child_process";

import { spawn } from "node:child_process";
import { once } from "node:events";
import { createInterface } from "node:readline";

interface RunProcessOptions {
  env?: NodeJS.ProcessEnv;
  timeoutMs: number;
}

export function spawnProcess(
  command: string,
  args: string[],
  options: { env?: NodeJS.ProcessEnv; inherit?: boolean } = {},
): ChildProcessWithoutNullStreams {
  const stdio = options.inherit ? "inherit" : "pipe";
  return spawn(command, args, {
    cwd: process.cwd(),
    env: options.env ?? process.env,
    stdio,
  }) as ChildProcessWithoutNullStreams;
}

export async function runProcess(
  command: string,
  args: string[],
  options: RunProcessOptions,
): Promise<void> {
  const child = spawn(command, args, {
    cwd: process.cwd(),
    detached: process.platform !== "win32",
    env: options.env ?? process.env,
    stdio: "inherit",
  });
  const exit = once(child, "exit") as Promise<[number | null, NodeJS.Signals | null]>;
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<"timeout">((resolveTimeout) => {
    timer = setTimeout(() => resolveTimeout("timeout"), options.timeoutMs);
  });
  const forwardInterrupt = (): void => signalProcess(child, "SIGINT");
  const forwardTermination = (): void => signalProcess(child, "SIGTERM");
  process.once("SIGINT", forwardInterrupt);
  process.once("SIGTERM", forwardTermination);
  let result: "timeout" | [number | null, NodeJS.Signals | null];
  try {
    result = await Promise.race([exit, timeout]);
  } finally {
    if (timer) clearTimeout(timer);
    process.off("SIGINT", forwardInterrupt);
    process.off("SIGTERM", forwardTermination);
  }

  if (result === "timeout") {
    await terminateProcessTree(child, exit);
    throw new Error(`${command} timed out after ${options.timeoutMs} ms.`);
  }

  const [code, signal] = result;
  if (code !== 0) {
    throw new Error(`${command} exited with ${signal ?? code ?? "an unknown status"}.`);
  }
}

function signalProcess(child: ReturnType<typeof spawn>, signal: NodeJS.Signals): void {
  if (child.exitCode !== null || child.signalCode !== null) return;
  if (process.platform !== "win32" && child.pid !== undefined) {
    try {
      process.kill(-child.pid, signal);
      return;
    } catch {
      // Fall back to the direct process below if the group is already gone.
    }
  }
  child.kill(signal);
}

async function terminateProcessTree(
  child: ReturnType<typeof spawn>,
  exit: Promise<[number | null, NodeJS.Signals | null]>,
): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;

  if (process.platform === "win32" && child.pid !== undefined) {
    await new Promise<void>((resolveKill) => {
      const killer = spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], {
        stdio: "ignore",
        windowsHide: true,
      });
      killer.once("error", () => {
        child.kill();
        resolveKill();
      });
      killer.once("exit", () => resolveKill());
    });
  } else if (child.pid !== undefined) {
    signalProcess(child, "SIGTERM");
  } else {
    child.kill();
  }

  const stopped = exit.then(() => true, () => true);
  let timer: NodeJS.Timeout | undefined;
  const graceful = new Promise<false>((resolveTimeout) => {
    timer = setTimeout(() => resolveTimeout(false), 5_000);
  });
  const exited = await Promise.race([stopped, graceful]);
  if (timer) clearTimeout(timer);
  if (exited) return;

  if (process.platform !== "win32" && child.pid !== undefined) {
    signalProcess(child, "SIGKILL");
  } else {
    child.kill("SIGKILL");
  }
  await exit.catch(() => undefined);
}

export async function firstOutputLine(
  child: ChildProcessWithoutNullStreams,
  timeoutMs: number,
): Promise<string> {
  const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error("Fixture process did not become ready in time.")), timeoutMs);
  });
  const exited = once(child, "exit").then(() => {
    throw new Error("Fixture process exited before publishing its manifest.");
  });

  try {
    const [line] = (await Promise.race([once(lines, "line"), exited, timeout])) as [string];
    return line;
  } finally {
    if (timer) clearTimeout(timer);
    lines.close();
  }
}

async function waitForChildExit(exit: Promise<unknown>, timeoutMs: number): Promise<boolean> {
  let timer: NodeJS.Timeout | undefined;
  const timedOut = new Promise<false>((resolveTimeout) => {
    timer = setTimeout(() => resolveTimeout(false), timeoutMs);
  });
  const exited = exit.then(() => true, () => true);
  const result = await Promise.race([exited, timedOut]);
  if (timer) clearTimeout(timer);
  return result;
}

async function forceStopFixture(
  child: ChildProcessWithoutNullStreams,
  timeoutMs: number,
): Promise<void> {
  if (process.platform !== "win32" || child.pid === undefined) {
    child.kill("SIGKILL");
    return;
  }

  await new Promise<void>((resolveKill) => {
    const killer = spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], {
      stdio: "ignore",
      windowsHide: true,
    });
    let settled = false;
    const finish = (): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolveKill();
    };
    const timer = setTimeout(() => {
      killer.kill();
      child.kill("SIGKILL");
      finish();
    }, timeoutMs);
    killer.once("error", finish);
    killer.once("exit", finish);
  });
}

export async function stopProcess(
  child: ChildProcessWithoutNullStreams,
  timeoutMs = 5_000,
): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.stdin.end();

  const exit = once(child, "exit");
  if (await waitForChildExit(exit, timeoutMs)) return;

  await forceStopFixture(child, timeoutMs);
  if (!(await waitForChildExit(exit, timeoutMs))) {
    throw new Error(`Fixture process did not stop within ${timeoutMs * 2} ms.`);
  }
}
