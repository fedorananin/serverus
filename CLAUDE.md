# Serverus

SSH/SFTP/FTP/S3 connection manager (macOS primary; Windows/Linux builds are
experimental). Replaces electerm + Cyberduck: terminal, dual-pane file
manager, and SSH tunnels in one window, with a single encrypted vault file
unlocked via master password / Touch ID / Windows Hello.

The original Russian spec (`docs/SPEC.md`) served its purpose and was removed
in v1.1.0 — it lives in git history. Comments citing "SPEC §n" refer to it;
keep them as historical anchors, don't add new ones. The behavior contract now
lives in this file, README.md, and the tests.

## Stack

- Tauri 2, Rust backend (`src-tauri/`), Svelte 5 + TypeScript frontend (`src/`)
- SSH/SFTP: `russh` + `russh-sftp`; FTP/FTPS: `suppaftp` (+ `rustls`);
  S3: `aws-sdk-s3` (custom endpoints — DO Spaces, R2, B2, MinIO); terminal: xterm.js
- Crypto: `argon2`, `aes-gcm`, `rand` — audited crates only, never hand-rolled crypto
- Keychain/Touch ID: `security-framework` + `objc2-local-authentication`
- Finder drag-out: `tauri-plugin-drag` (`@crabnebula/tauri-plugin-drag` on the JS side)

## Commands

```bash
npm run verify         # canonical local gate: architecture, bindings, TS, tests, build, Rust
npm run tauri dev      # run the app in dev mode
npm run tauri build    # release build → ~/.cache/serverus-target/release/bundle/
npm run architecture:check
cargo test --workspace # unit + integration; needs sandbox OFF for the sshd tests
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
- OS-specific code stays isolated: quick unlock is a trait (`QuickUnlock`)
  with macOS (Keychain + Touch ID) and Windows (KeyCredentialManager /
  Windows Hello) impls and a no-op fallback on Linux; everything else is
  `cfg`-gated in place (`local_fs` permissions, URL/editor opening, default
  terminal font). The frontend maps ⌘→Ctrl via `src/lib/platform.ts` —
  never test `e.metaKey` directly in components.

## Code style

- English everywhere: identifiers, comments, commit messages, UI strings.
- Rust: rustfmt defaults, clippy clean with `-D warnings`.
- TypeScript: strict mode; Tauri command types generated via `tauri-specta`
  (never hand-duplicate types between Rust and TS).
- Svelte components small and focused; shared state lives in `src/lib/stores/`.
- New Tauri command slices are mapping-only: parse DTO → call the application
  handle → map the result. Legacy `commands.rs` still contains orchestration
  and is migrated incrementally; do not copy that orchestration into new code.

## Architecture map (where things live)

- **Rust is a workspace.** `crates/serverus-domain` is the effect-free core;
  `serverus-application` owns use cases/ports; `serverus-runtime` owns context
  generations and is the target home for long-lived supervisors;
  `serverus-adapters` contains extracted port implementations;
  `serverus-testkit` contains the reusable fakes extracted so far. `src-tauri`
  is the transitional desktop composition root and still owns legacy managers,
  workers, and protocol adapters during migration.
- **TS bindings are generated, never hand-written.** `src/lib/api/bindings.ts`
  is produced only by `npm run bindings:generate`; `npm run bindings:check`
  regenerates and fails on drift. Ordinary startup and tests never write it.
  After adding/changing a `#[tauri::command]` or `#[derive(Type)]`, regenerate.
  `u64`/`i64` fields need `#[specta(type = specta_typescript::Number)]` or the
  export panics (specta forbids BigInt types by default).
- **Frontend state:** migrated features use the app-scoped model in
  `src/lib/app/` with injected `AppApi` / `AppEventSource`; Transfers is the
  first migrated slice. Remaining legacy Svelte stores are module singletons
  and must be migrated feature by feature rather than copied.
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
- Transfer queue + history are **per-session, end to end**: the panel lives in
  each tab's Files view and shows only that session's transfers (per-session
  summaries + bulk actions scoped by `session_id`); cleared on tab
  close/disconnect (`TransferManager::clear_session`) and on app exit. There is
  no global transfer panel.
- `Connection.disable_terminal` = SFTP-only SSH servers (no shell); the UI hides
  the terminal view and the backend never opens a shell channel.

## Project status

All milestones M0–M7 implemented (v1.0.0): vault + Touch ID, connection
manager, SSH terminal (multiple per tab), dual-pane SFTP/FTP file manager with
per-connection transfer queue, tar acceleration, remote edit, tunnels, jump
hosts, auto-lock. Post-M7 additions: pointer-based drag-and-drop with Finder
in/out, cleartext secrets in the edit form, SFTP-only connections, and S3
support — any S3-compatible endpoint, prefixes-as-folders,
multipart uploads, public/private ACLs with background badge loading, an
upload-ACL mode switch (private/public/ask), and "Copy public URL".
v1.1.0: config import (`vault/import.rs`, format documented in
`docs/CONFIG_FORMAT.md`), SSH key-file → vault-text import, native pickers for
the vault path and config import, folder badges applied at creation, and the
folder item count shown only while collapsed. v1.1.2: the sidebar is
drag-resizable (200–380 px, default 230, double-click the edge to reset) —
the width lives in `PanelSettings::sidebar_width`, clamped both in the drag
handler and in `Settings::clamp` on write; folders persist their disclosure
state in `TreeNode::Folder::collapsed` (`serde(default)` = expanded, so old
vaults and imports open as before). Also v1.1.0: cross-platform
support — Windows Hello quick unlock (`quick_unlock.rs::windows_hello`,
KeePassXC scheme: Hello-gated deterministic RSA signature → HKDF → AES-GCM
wrapped DEK blob in the config dir), cfg-gated unix permissions / openers /
fonts, ⌘→Ctrl shortcut mapping. **Windows/Linux are experimental:** hosted
CI builds all three OSes and exercises supported desktop scenarios through
WKWebView, WebKitGTK and WebView2, but this is not representative physical
Windows/Linux hardware validation.
Known gaps: no Linux quick unlock; lock-on-sleep detection (monotonic vs wall
clock divergence) may not fire on Windows; local chmod is hidden on Windows.
Integration tests (32) run against a local unprivileged `sshd`, an in-process
libunftp FTP server and an in-process `s3s` S3 server — no docker needed
(macOS + Linux; Windows runs `cargo test --workspace --lib`, plus supported
non-SSH desktop scenarios). Releases are built by
`.github/workflows/release.yml` on `v*` tags: dmg / msi+nsis /
AppImage+deb+rpm attached to a draft GitHub Release. Personal use for now;
the repo is written to open-source quality (MIT, CI in
`.github/workflows/ci.yml`).

## Notes

- The project lives in OneDrive. The Cargo build cache is redirected outside
  it via `.cargo/config.toml` (machine-local, gitignored) to
  `~/.cache/serverus-target` — do not remove that file, and never place
  `target/` inside the project tree. `node_modules/` stays gitignored.
- Integration tests run against a local unprivileged `sshd` (bundled with
  macOS/CI) and an in-process libunftp server — no docker. See
  `src-tauri/tests/support/mod.rs`.
