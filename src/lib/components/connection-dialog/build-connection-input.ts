import type {
  AuthMethod,
  Badge,
  ConnectionInput,
  FtpTlsMode,
  Protocol,
  S3UploadAcl,
  TunnelConfig,
} from "$lib/api";

export interface ConnectionDraft {
  name: string;
  badge: Badge | null;
  protocol: Protocol;
  host: string;
  port: number;
  authMethod: AuthMethod;
  username: string;
  password: string;
  keyPath: string;
  keySource: "file" | "text";
  keyInline: string;
  keyPassphrase: string;
  jumpHost: string | null;
  ftpTls: FtpTlsMode;
  ftpPassive: boolean;
  s3Region: string;
  s3Bucket: string;
  s3PathStyle: boolean;
  s3PublicBaseUrl: string;
  s3UploadAcl: S3UploadAcl;
  remoteDir: string;
  localDir: string;
  tunnels: TunnelConfig[];
  disableTerminal: boolean;
  notes: string;
}

function optional(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

export function buildConnectionInput(draft: ConnectionDraft): ConnectionInput {
  const usesInlineKey =
    draft.protocol === "ssh" && draft.authMethod === "key" && draft.keySource === "text";

  return {
    name: draft.name.trim(),
    badge: draft.badge,
    protocol: draft.protocol,
    host: draft.host.trim(),
    port: draft.port,
    auth_method: draft.authMethod,
    username: draft.username.trim(),
    password: draft.password,
    key_path: usesInlineKey ? null : optional(draft.keyPath),
    key_inline:
      draft.protocol === "ssh" && draft.authMethod === "key"
        ? draft.keySource === "text"
          ? draft.keyInline
          : ""
        : null,
    key_passphrase: draft.keyPassphrase,
    jump_host: draft.protocol === "ssh" ? draft.jumpHost : null,
    ftp:
      draft.protocol === "ftp"
        ? { tls: draft.ftpTls, passive: draft.ftpPassive }
        : null,
    s3:
      draft.protocol === "s3"
        ? {
            region: optional(draft.s3Region),
            bucket: optional(draft.s3Bucket),
            path_style: draft.s3PathStyle,
            public_base_url: optional(draft.s3PublicBaseUrl),
            upload_acl: draft.s3UploadAcl,
          }
        : null,
    remote_dir: optional(draft.remoteDir),
    local_dir: optional(draft.localDir),
    tunnels: draft.protocol === "ssh" ? draft.tunnels : [],
    disable_terminal: draft.protocol === "ssh" ? draft.disableTerminal : false,
    notes: draft.notes,
  };
}
