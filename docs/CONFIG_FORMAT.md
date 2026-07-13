# Config export / import format

Serverus can export its whole configuration to a plain JSON file
(**Settings → Vault → Export config**) and import one back
(**Settings → Vault → Import config**). The same format works both ways, and
it is deliberately forgiving on import — you can write a file by hand to
migrate from another app (Cyberduck, electerm, `~/.ssh/config`, a spreadsheet…)
without scripting anything.

> **Security note.** An export is **unencrypted** and never contains secrets —
> no passwords, passphrases or private keys, only `has_password`-style flags.
> An import, however, **may** carry secrets: they go straight into the
> encrypted vault, so putting them in a hand-written file for a one-time
> migration is fine. Delete the file afterwards.

## Shape

Every top-level section is optional. Unknown fields are ignored (that's why a
Serverus export, which contains a few UI-only flags, imports cleanly).

```jsonc
{
  "tree": [ /* sidebar layout: folders and connection refs */
    {
      "type": "folder",
      "id": "any-unique-string",      // optional — generated when missing
      "name": "Clients",
      "badge": { "kind": "emoji", "value": "💼" },   // or { "kind": "color", "value": "#e5484d" }
      "children": [ { "type": "connection", "id": "web-1" } ]
    },
    { "type": "connection", "id": "backup-ftp" }
  ],

  "connections": {                    // keyed by id — any unique string works
    "web-1": {
      "name": "prod web",             // optional — defaults to host
      "badge": { "kind": "color", "value": "#46a758" },   // optional
      "protocol": "ssh",              // REQUIRED: "ssh" | "ftp" | "s3"
      "host": "web.example.com",      // REQUIRED (S3: the endpoint)
      "port": 22,                     // optional — 22 / 21 / 443 by protocol
      "auth": {
        "method": "key",              // optional: "password" | "key" | "agent";
                                      // defaults to "key" when a key is given,
                                      // "password" otherwise
        "username": "deploy",
        "password": "…",              // secrets are OPTIONAL — see note above
        "key_path": "~/.ssh/id_ed25519",          // key as a file on disk…
        "key_inline": "-----BEGIN OPENSSH PRIVATE KEY-----\n…",  // …or as text in the vault (recommended: survives machine moves)
        "key_passphrase": "…"
      },
      "jump_host": "bastion-id",      // id of another connection, or omit
      "remote_dir": "/var/www",       // optional start dirs
      "local_dir": "~/Projects/site",
      "tunnels": [
        { "name": "mysql", "kind": "local", "local_port": 3306,
          "remote_host": "127.0.0.1", "remote_port": 3306, "autostart": false }
      ],
      "disable_terminal": false,      // true = SFTP-only account (no shell)
      "notes": "free text"
    },

    "backup-ftp": {
      "protocol": "ftp",
      "host": "ftp.example.com",
      "auth": { "username": "backup", "password": "…" },
      "ftp": { "tls": "explicit", "passive": true }   // tls: "none" | "explicit"
    },

    "cdn": {
      "protocol": "s3",
      "host": "fra1.digitaloceanspaces.com",
      "auth": { "username": "ACCESS_KEY_ID", "password": "SECRET_ACCESS_KEY" },
      "s3": {
        "region": "fra1",             // optional signing region
        "bucket": "my-space",         // optional — omit to browse all buckets
        "path_style": false,          // true for MinIO/self-hosted
        "public_base_url": "https://cdn.example.com",   // optional CDN base for "Copy public URL"
        "upload_acl": "private"       // "private" | "public_read" | "ask"
      }
    }
  },

  "known_hosts": { "web.example.com:22": "ssh-ed25519 AAAA…" },

  "settings": { /* the full app settings block, as written by an export.
                   Omit it to keep your current settings. */ }
}
```

The smallest valid file is one connection:

```json
{ "connections": { "my-server": { "protocol": "ssh", "host": "1.2.3.4",
                                  "auth": { "username": "root", "password": "hunter2" } } } }
```

## Import semantics

Import **merges** into the current vault — nothing is deleted:

- **Connections** are upserted by id: a new id is added, an existing id is
  overwritten field-by-field. Secrets already stored in the vault are **kept**
  when the file doesn't provide them — so re-importing an (secret-free) export
  over a live vault does not wipe your passwords.
- **Tree**: imported folders/refs are appended to the sidebar; nodes the
  import re-places are moved, not duplicated — importing the same file twice
  is a no-op. Imported connections not mentioned in `tree` appear at the root.
  Refs to unknown connection ids are dropped; a `jump_host` pointing nowhere
  is detached.
- **Known hosts** merge; on conflict the existing (already verified) entry wins.
- **Settings**, when present, replace the current ones wholesale.

Only Serverus's own format is supported — there are no importers for other
apps' files; convert them into this shape instead.
