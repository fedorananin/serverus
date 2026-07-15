# Current Limitations

This document distinguishes the implemented v1.1.0 product from adjacent
features that a connection manager might otherwise imply.

## Platform maturity

- macOS is the primary platform.
- Windows and Linux builds are experimental. Hosted GitHub runners exercise
  their native WebViews (WebView2 and WebKitGTK) with the supported desktop
  scenario catalog, but this does not substitute for testing native dialogs,
  biometrics, installed editors, display stacks, or hardware diversity on
  representative physical machines.
- Windows runs the supported non-SSH desktop scenarios, while its Rust test
  step is limited to `cargo test --workspace --lib`; the Unix-only Rust
  integration binaries and SSH-backed desktop scenarios require a Windows
  OpenSSH fixture before they can become Windows regression gates.
- Linux has no quick-unlock implementation.
- Lock-on-sleep detection may not reliably observe sleep on Windows because of
  platform clock behavior.
- Local Unix permission editing is hidden on Windows.
- Release binaries are not currently presented as fully signed/notarized
  production distributions on every platform.

## Access and collaboration

- There is no Serverus user account, cloud backend, team workspace, or RBAC.
- There is no master-password recovery flow.
- Serverus does not provide built-in vault synchronization. A user may copy or
  externally synchronize the single vault file at their own risk.
- Concurrent editing of the same vault by several application instances is not
  a collaborative workflow.

## Protocols and networking

- FTP supports plain FTP and explicit FTPS, not implicit FTPS.
- There is no dedicated SCP mode.
- WebDAV is not supported.
- SSH tunnels implement local port forwarding only. Remote forwarding and
  dynamic SOCKS proxy are not supported.
- Server-to-server file transfer is not supported; transfers always have the
  local machine as one endpoint.
- There is no two-way directory synchronization or directory-diff workflow.

## Transfer persistence and protocol differences

- Transfer queues and histories are not durable across tab closure or
  application restart.
- S3 uploads restart rather than resume after failure.
- S3 object modification time cannot be restored to an arbitrary source mtime.
- S3 ACL state and ACL operations depend on provider support.
- Tar acceleration applies only to eligible SSH/SFTP directory transfers.
- A tar transfer falls back to ordinary per-file transfer after the failed item
  is retried; there is no separate pre-transfer "force plain" control.
- Drag-and-drop expresses copy/upload/download operations. The product does not
  expose a general cross-pane move modifier.

## Import and product surface

- Import accepts the documented Serverus JSON format, including hand-written
  conversion files, but does not directly parse Electerm, Cyberduck, or
  `~/.ssh/config` formats.
- The UI is English-only.
- There is no built-in automatic updater in the documented v1.1.0 behavior.
