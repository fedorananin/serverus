import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { parseFixtureManifest } from "./fixture-manifest";

const validManifest = {
  paths: {
    workspace_root: "/tmp/workspace",
    app_config_dir: "/tmp/config",
    vault_dir: "/tmp/vaults",
    local_source: "/tmp/source",
    local_download: "/tmp/download",
    ftp_root: "/tmp/ftp",
    s3_root: "/tmp/s3",
    ssh_root: "/tmp/ssh",
  },
  ftp: { host: "127.0.0.1", port: 21000, username: "anonymous" },
  s3: { endpoint: "http://127.0.0.1:22000", port: 22000 },
  ssh: { available: false },
  editor: { executable: "/tmp/serverus-e2e-fixtures" },
};

describe("parseFixtureManifest", () => {
  it("accepts the fixture process contract", () => {
    const parsed = parseFixtureManifest(JSON.stringify(validManifest));

    assert.equal(parsed.paths.app_config_dir, "/tmp/config");
    assert.equal(parsed.ftp.port, 21000);
    assert.equal(parsed.ssh.available, false);
    assert.equal(parsed.editor.executable, "/tmp/serverus-e2e-fixtures");
  });

  it("accepts an available SSH fixture without exposing key contents", () => {
    const parsed = parseFixtureManifest(
      JSON.stringify({
        ...validManifest,
        ssh: {
          available: true,
          host: "127.0.0.1",
          port: 23000,
          username: "runner",
          key_path: "/tmp/id_ed25519",
        },
      }),
    );

    assert.equal(parsed.ssh.available, true);
    if (parsed.ssh.available) assert.equal(parsed.ssh.key_path, "/tmp/id_ed25519");
  });

  it("fails fast when the process emits malformed or sensitive data", () => {
    assert.throws(() => parseFixtureManifest("not json"), /valid JSON/);
    assert.throws(
      () => parseFixtureManifest(JSON.stringify({ ...validManifest, password: "do-not-print" })),
      /sensitive field/i,
    );
    assert.throws(
      () => parseFixtureManifest(JSON.stringify({ ...validManifest, token: "do-not-forward" })),
      /sensitive field/i,
    );
    assert.throws(
      () =>
        parseFixtureManifest(
          JSON.stringify({
            ...validManifest,
            ftp: { ...validManifest.ftp, display_name: "unexpected" },
          }),
        ),
      /unexpected field/i,
    );
    assert.throws(
      () => parseFixtureManifest(JSON.stringify({ ...validManifest, ftp: { port: 0 } })),
      /fixture manifest/i,
    );
  });
});
