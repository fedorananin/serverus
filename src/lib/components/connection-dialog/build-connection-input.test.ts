import { describe, expect, it } from "vitest";

import type { ConnectionDraft } from "./build-connection-input";
import { buildConnectionInput } from "./build-connection-input";

const baseDraft: ConnectionDraft = {
  name: " Production ",
  badge: null,
  protocol: "ssh",
  host: " prod.example.com ",
  port: 22,
  authMethod: "password",
  username: " deploy ",
  password: "secret",
  keyPath: "",
  keySource: "file",
  keyInline: "",
  keyPassphrase: "",
  jumpHost: null,
  ftpTls: "none",
  ftpPassive: true,
  s3Region: "",
  s3Bucket: "",
  s3PathStyle: false,
  s3PublicBaseUrl: "",
  s3UploadAcl: "private",
  remoteDir: " /srv/app ",
  localDir: " ~/Projects/app ",
  tunnels: [],
  disableTerminal: false,
  notes: "kept verbatim  ",
};

describe("buildConnectionInput", () => {
  it("keeps SSH inline-key, jump-host, tunnel and terminal contracts", () => {
    const draft: ConnectionDraft = {
      ...baseDraft,
      authMethod: "key",
      keyPath: "/unused/id_ed25519",
      keySource: "text",
      keyInline: "PRIVATE KEY\n",
      keyPassphrase: "key-secret",
      jumpHost: "bastion-id",
      disableTerminal: true,
      tunnels: [
        {
          name: "database",
          kind: "local",
          local_port: 5432,
          remote_host: "127.0.0.1",
          remote_port: 5432,
          autostart: true,
        },
      ],
    };

    expect(buildConnectionInput(draft)).toEqual({
      name: "Production",
      badge: null,
      protocol: "ssh",
      host: "prod.example.com",
      port: 22,
      auth_method: "key",
      username: "deploy",
      password: "secret",
      key_path: null,
      key_inline: "PRIVATE KEY\n",
      key_passphrase: "key-secret",
      jump_host: "bastion-id",
      ftp: null,
      s3: null,
      remote_dir: "/srv/app",
      local_dir: "~/Projects/app",
      tunnels: draft.tunnels,
      disable_terminal: true,
      notes: "kept verbatim  ",
    });
  });

  it("keeps FTP options and removes SSH-only values", () => {
    const draft: ConnectionDraft = {
      ...baseDraft,
      protocol: "ftp",
      port: 21,
      ftpTls: "explicit",
      ftpPassive: false,
      jumpHost: "ignored",
      tunnels: [
        {
          name: "ignored",
          kind: "local",
          local_port: 1,
          remote_host: "ignored",
          remote_port: 2,
          autostart: false,
        },
      ],
      disableTerminal: true,
    };

    expect(buildConnectionInput(draft)).toMatchObject({
      protocol: "ftp",
      ftp: { tls: "explicit", passive: false },
      s3: null,
      jump_host: null,
      tunnels: [],
      disable_terminal: false,
    });
  });

  it("normalizes optional S3 values without losing upload access", () => {
    const draft: ConnectionDraft = {
      ...baseDraft,
      protocol: "s3",
      port: 443,
      s3Region: "  ",
      s3Bucket: " assets ",
      s3PathStyle: true,
      s3PublicBaseUrl: " https://cdn.example.com/root ",
      s3UploadAcl: "ask",
    };

    expect(buildConnectionInput(draft)).toMatchObject({
      protocol: "s3",
      ftp: null,
      s3: {
        region: null,
        bucket: "assets",
        path_style: true,
        public_base_url: "https://cdn.example.com/root",
        upload_acl: "ask",
      },
      jump_host: null,
      tunnels: [],
      disable_terminal: false,
    });
  });
});
