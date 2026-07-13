# Serverus

SSH/SFTP/FTP/S3 connection manager for macOS. Replaces electerm + Cyberduck:
terminal, dual-pane file manager, and SSH tunnels in one window, with a single
encrypted vault file unlocked via master password / Touch ID.

**The full spec is the source of truth: [docs/SPEC.md](docs/SPEC.md).**
Read it before implementing any feature. Do not silently deviate from it —
if a spec decision turns out to be wrong or impractical, raise it and update
the spec in the same change.

## Stack

- Tauri 2, Rust backend (`src-tauri/`), Svelte 5 + TypeScript frontend (`src/`)
- SSH/SFTP: `russh` + `russh-sftp`; FTP/FTPS: `suppaftp` (+ `rustls`);
  S3: `aws-sdk-s3` (custom endpoints — DO Spaces, R2, B2, MinIO); terminal: xterm.js
- Crypto: `argon2`, `aes-gcm`, `rand` — audited crates only, never hand-rolled crypto
- Keychain/Touch ID: `security-framework` + `objc2-local-authentication`
- Finder drag-out: `tauri-plugin-drag` (`@crabnebula/tauri-plugin-drag` on the JS side)

## Commands

```bash
npm run tauri dev      # run the app in dev mode
npm run tauri build    # release build → ~/.cache/serverus-target/release/bundle/
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test  --manifest-path src-tauri/Cargo.toml   # unit + integration; needs sandbox OFF
npm run check          # svelte-check + tsc
```

Integration tests spawn a real unprivileged `sshd` whose sftp-server does
chmod/rename — the macOS seatbelt sandbox blocks those, so run tests with the
Bash sandbox disabled (symptom otherwise: "Permission denied" on chmod/rename).

## Hard rules

- **Security first.** Secrets (passwords, passphrases, DEK) never appear in
  logs, error messages, Tauri command return values beyond what the UI strictly
  needs, or debug output. Zeroize secret buffers on lock/drop. The master
  password is never persisted anywhere.
- **Vault integrity.** All vault writes are atomic (temp file + rename) with a
  `.bak` copy of the previous version. Never write the payload unencrypted to disk.
- **Protocol abstraction.** File operations go through the `RemoteFs` trait;
  UI and the transfer queue must not know whether a session is SFTP or FTP.
- **Recursive FTP directory transfers must always work** — this is the founding
  pain point of the project (electerm bug). Any change touching transfers must
  keep the recursive-FTP integration test green.
- macOS-specific code (Keychain, Touch ID, FSEvents) stays isolated behind
  traits — future Linux/Windows ports must not require rewrites.

## Code style

- English everywhere: identifiers, comments, commit messages, UI strings.
- Rust: rustfmt defaults, clippy clean with `-D warnings`.
- TypeScript: strict mode; Tauri command types generated via `tauri-specta`
  (never hand-duplicate types between Rust and TS).
- Svelte components small and focused; shared state lives in `src/lib/stores/`.
- Tauri commands in `commands.rs` are thin: parse input → call a module → return.
  Business logic lives in modules: `vault/` (crypto, model, quick-unlock, tree),
  `session/` (`ssh`, `sftp`, `ftp`, `s3`, `remote_fs` trait, `tunnel`), `transfer/`
  (queue + `tar_stream`), `watcher/` (remote edit), `autolock`, `local_fs`.

## Architecture map (where things live)

- **TS bindings are generated, never hand-written.** `src/lib/api/bindings.ts`
  is produced by `cargo test export_bindings --lib` (also on every `tauri dev`).
  After adding/changing a `#[tauri::command]` or `#[derive(Type)]`, regenerate.
  `u64`/`i64` fields need `#[specta(type = specta_typescript::Number)]` or the
  export panics (specta forbids BigInt types by default).
- **Frontend state** (`src/lib/stores/*.svelte.ts`, Svelte 5 runes): `vault`
  (unlock ↔ main screen, holds the secret-free `PublicVault`), `tabs` (one
  session per tab, auto-reconnect), `transfers`, `dnd` (pointer-drag state),
  `hostkey`, `pane` (one per file panel).
- **Secrets never reach the frontend by default** — `PublicVault` /
  `PublicConnection` redact them. The edit form pulls real values on demand via
  the `connection_secrets` command (safe: vault already unlocked) and shows them
  in cleartext.

## Gotchas & conventions

- **HTML5 drag-and-drop does not work inside the Tauri WKWebView** — the native
  file-drop handler intercepts it. In-app dragging is pointer-event based
  (`src/lib/stores/dnd.svelte.ts` + a ghost). File panes: the local pane uses
  the OS-native drag (`startDrag`) so files can go OUT to Finder; drops landing
  back in the window arrive via Tauri's `onDragDropEvent` and are routed by
  cursor position. Remote pane uses pointer-drag (remote files aren't on disk).
  Never reach for `draggable`/`dataTransfer`.
- **Terminal views stay mounted when hidden** (SessionView toggles `display`),
  because unmounting closes the SSH channel and loses the shell. Guard
  `FitAddon.fit()` against 0×0.
- **`russh-sftp`: `FileAttributes::default()` is NOT empty** (carries uid/gid 0
  → SETSTAT chowns to root). Build from `FileAttributes::empty()` for chmod/mtime.
- Transfer queue + history are **per-connection**: cleared on tab
  close/disconnect (`TransferManager::clear_session`) and on app exit.
- `Connection.disable_terminal` = SFTP-only SSH servers (no shell); the UI hides
  the terminal view and the backend never opens a shell channel.

## Project status

All milestones M0–M7 implemented (v1.0.0): vault + Touch ID, connection
manager, SSH terminal (multiple per tab), dual-pane SFTP/FTP file manager with
per-connection transfer queue, tar acceleration, remote edit, tunnels, jump
hosts, auto-lock. Post-M7 additions: pointer-based drag-and-drop with Finder
in/out, cleartext secrets in the edit form, SFTP-only connections, and S3
support (SPEC §4.4) — any S3-compatible endpoint, prefixes-as-folders,
multipart uploads, public/private ACLs with background badge loading, an
upload-ACL mode switch (private/public/ask), and "Copy public URL".
Integration tests (32) run against a local unprivileged `sshd`, an in-process
libunftp FTP server and an in-process `s3s` S3 server (SPEC §7.3) — no docker
needed. Personal use for now; the repo is written to open-source quality
(MIT, CI in `.github/workflows/ci.yml`).

## Notes

- The project lives in OneDrive. The Cargo build cache is redirected outside
  it via `.cargo/config.toml` (machine-local, gitignored) to
  `~/.cache/serverus-target` — do not remove that file, and never place
  `target/` inside the project tree. `node_modules/` stays gitignored.
- Integration tests run against a local unprivileged `sshd` (bundled with
  macOS/CI) and an in-process libunftp server — no docker. See SPEC.md §7.3
  and `src-tauri/tests/support/mod.rs`.
