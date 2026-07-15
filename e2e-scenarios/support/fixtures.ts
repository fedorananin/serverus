import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, rename, rm } from "node:fs/promises";
import { join } from "node:path";
import { promisify } from "node:util";

import { parseFixtureManifest, type FixtureManifest } from "../src/fixture-manifest";

let cached: FixtureManifest | undefined;
const execFileAsync = promisify(execFile);

export interface SshHostKeyIdentity {
  algorithm: string;
  fingerprint: string;
}

export function fixtures(): FixtureManifest {
  cached ??= parseFixtureManifest(
    process.env.SERVERUS_SCENARIO_FIXTURE_MANIFEST ?? "",
  );
  return cached;
}

function availableSshFixture(): Extract<FixtureManifest["ssh"], { available: true }> {
  const ssh = fixtures().ssh;
  if (!ssh.available) throw new Error("The SSH scenario fixture is unavailable on this platform.");
  return ssh;
}

function parseHostKeyIdentity(publicKey: string): SshHostKeyIdentity {
  const line = publicKey
    .split(/\r?\n/u)
    .map((candidate) => candidate.trim())
    .find((candidate) => candidate.length > 0 && !candidate.startsWith("#"));
  const fields = line?.split(/\s+/u) ?? [];
  const keyOffset = fields[0]?.startsWith("ssh-") ? 0 : 1;
  const [algorithm, encodedKey] = fields.slice(keyOffset, keyOffset + 2);
  if (!algorithm || !encodedKey) throw new Error("SSH fixture returned an invalid public key.");

  const digest = createHash("sha256").update(Buffer.from(encodedKey, "base64")).digest("base64");
  return { algorithm, fingerprint: `SHA256:${digest.replace(/=+$/u, "")}` };
}

function sshRuntimePath(name: string): string {
  return join(fixtures().paths.workspace_root, ".ssh-fixture", name);
}

export async function currentSshHostKeyIdentity(): Promise<SshHostKeyIdentity> {
  availableSshFixture();
  return parseHostKeyIdentity(await readFile(sshRuntimePath("host_ed25519.pub"), "utf8"));
}

async function scanSshHostKey(): Promise<SshHostKeyIdentity> {
  const ssh = availableSshFixture();
  const { stdout } = await execFileAsync(
    "ssh-keyscan",
    ["-T", "2", "-p", String(ssh.port), ssh.host],
    { encoding: "utf8" },
  );
  return parseHostKeyIdentity(stdout);
}

async function waitForSshHostKey(expected: SshHostKeyIdentity): Promise<SshHostKeyIdentity> {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    try {
      const observed = await scanSshHostKey();
      if (observed.fingerprint === expected.fingerprint) return observed;
    } catch {
      // sshd briefly closes its listener while reloading the replacement key.
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error("SSH fixture did not publish its rotated host key in time.");
}

export async function rotateSshHostKey(): Promise<SshHostKeyIdentity> {
  availableSshFixture();
  const replacement = sshRuntimePath(`host_ed25519.next-${process.pid}-${Date.now()}`);
  try {
    await execFileAsync(
      "ssh-keygen",
      ["-q", "-t", "ed25519", "-N", "", "-C", "serverus-scenario-rotated", "-f", replacement],
      { encoding: "utf8" },
    );
    await rename(`${replacement}.pub`, sshRuntimePath("host_ed25519.pub"));
    await rename(replacement, sshRuntimePath("host_ed25519"));
    const expected = await currentSshHostKeyIdentity();
    const pid = Number((await readFile(sshRuntimePath("sshd.pid"), "utf8")).trim());
    if (!Number.isInteger(pid) || pid < 1) throw new Error("SSH fixture returned an invalid pid.");
    process.kill(pid, "SIGHUP");
    return await waitForSshHostKey(expected);
  } finally {
    await Promise.all([rm(replacement, { force: true }), rm(`${replacement}.pub`, { force: true })]);
  }
}

// Deliberately fixed credentials for the disposable in-process S3 fixture.
// Keep them out of manifests, logs, screenshots, and assertion messages.
export const S3_TEST_ACCESS_KEY = "serverus-e2e-access";
export const S3_TEST_SECRET_KEY = "serverus-e2e-secret";
