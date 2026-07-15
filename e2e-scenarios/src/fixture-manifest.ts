export interface FixturePaths {
  workspace_root: string;
  app_config_dir: string;
  vault_dir: string;
  local_source: string;
  local_download: string;
  ftp_root: string;
  s3_root: string;
  ssh_root: string;
}

export interface FixtureManifest {
  paths: FixturePaths;
  ftp: { host: string; port: number; username: string };
  s3: { endpoint: string; port: number };
  ssh:
    | { available: false }
    | { available: true; host: string; port: number; username: string; key_path: string };
  editor: { executable: string };
}

function record(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("Invalid fixture manifest shape.");
  }
  return value as Record<string, unknown>;
}

function assertOnlyFields(
  value: Record<string, unknown>,
  allowedFields: readonly string[],
  context: string,
): void {
  const allowed = new Set(allowedFields);
  for (const field of Object.keys(value)) {
    if (!allowed.has(field)) {
      throw new Error(`Fixture manifest ${context} has an unexpected field: ${field}.`);
    }
  }
}

function text(value: unknown): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error("Invalid fixture manifest string field.");
  }
  return value;
}

function port(value: unknown): number {
  if (!Number.isInteger(value) || Number(value) < 1 || Number(value) > 65_535) {
    throw new Error("Invalid fixture manifest port.");
  }
  return Number(value);
}

function assertNoSensitiveFields(value: unknown): void {
  if (Array.isArray(value)) {
    value.forEach(assertNoSensitiveFields);
    return;
  }
  if (typeof value !== "object" || value === null) return;

  for (const [key, child] of Object.entries(value)) {
    if (
      /password|secret|credential|token|passphrase|(^|[_-])dek($|[_-])|access[_-]?key|private[_-]?key$/i.test(
        key,
      )
    ) {
      throw new Error(`Fixture manifest contains a sensitive field: ${key}.`);
    }
    assertNoSensitiveFields(child);
  }
}

export function parseFixtureManifest(output: string): FixtureManifest {
  let parsed: unknown;
  try {
    parsed = JSON.parse(output);
  } catch {
    throw new Error("Fixture process did not return valid JSON.");
  }
  assertNoSensitiveFields(parsed);

  const manifest = record(parsed);
  assertOnlyFields(manifest, ["paths", "ftp", "s3", "ssh", "editor"], "root");
  const rawPaths = record(manifest.paths);
  assertOnlyFields(
    rawPaths,
    [
      "workspace_root",
      "app_config_dir",
      "vault_dir",
      "local_source",
      "local_download",
      "ftp_root",
      "s3_root",
      "ssh_root",
    ],
    "paths",
  );
  const paths: FixturePaths = {
    workspace_root: text(rawPaths.workspace_root),
    app_config_dir: text(rawPaths.app_config_dir),
    vault_dir: text(rawPaths.vault_dir),
    local_source: text(rawPaths.local_source),
    local_download: text(rawPaths.local_download),
    ftp_root: text(rawPaths.ftp_root),
    s3_root: text(rawPaths.s3_root),
    ssh_root: text(rawPaths.ssh_root),
  };

  const rawFtp = record(manifest.ftp);
  assertOnlyFields(rawFtp, ["host", "port", "username"], "ftp");
  const ftp = {
    host: text(rawFtp.host),
    port: port(rawFtp.port),
    username: text(rawFtp.username),
  };
  const rawS3 = record(manifest.s3);
  assertOnlyFields(rawS3, ["endpoint", "port"], "s3");
  const s3 = { endpoint: text(rawS3.endpoint), port: port(rawS3.port) };
  const rawSsh = record(manifest.ssh);
  assertOnlyFields(
    rawSsh,
    rawSsh.available === true
      ? ["available", "host", "port", "username", "key_path"]
      : ["available"],
    "ssh",
  );
  const ssh =
    rawSsh.available === true
      ? {
          available: true as const,
          host: text(rawSsh.host),
          port: port(rawSsh.port),
          username: text(rawSsh.username),
          key_path: text(rawSsh.key_path),
        }
      : rawSsh.available === false
        ? ({ available: false } as const)
        : (() => {
            throw new Error("Invalid fixture manifest SSH availability.");
          })();
  const rawEditor = record(manifest.editor);
  assertOnlyFields(rawEditor, ["executable"], "editor");
  const editor = { executable: text(rawEditor.executable) };

  return { paths, ftp, s3, ssh, editor };
}
