# Acceptance Criteria

These scenarios are the minimum end-to-end acceptance set for the v1.1.0
business requirements. Lower-level protocol and security tests remain part of
the executable contract.

## AC-001: Create and unlock a vault

**Given** no vault has been configured,

**when** the operator chooses a path, enters matching master passwords, and
creates the vault,

**then** the encrypted vault is created and the empty connection workspace is
opened.

The UI must state that a forgotten master password cannot be recovered.

## AC-002: Quick unlock

**Given** a vault has been unlocked with its master password and the operating
system supports quick unlock,

**when** the operator enables Touch ID or Windows Hello, locks the application,
and passes the platform biometric check,

**then** the same vault opens without entering its master password.

## AC-003: Unknown or changed SSH host key

**Given** an SSH server has no matching known-host record,

**when** the operator connects,

**then** the host, algorithm, and fingerprint are shown and the connection does
not proceed until the operator explicitly accepts the key.

If a saved key later changes, the warning must be visually more severe and a
rejection must stop the connection.

## AC-004: Multiple independent sessions

**Given** one saved connection,

**when** the operator opens it in two tabs and closes one tab,

**then** the other tab remains connected and usable.

## AC-005: Multiple terminal channels

**Given** a connected shell-enabled SSH tab,

**when** the operator creates multiple terminals, switches between them, and
closes one terminal,

**then** the remaining terminals and the SSH session continue running.

## AC-006: Recursive FTP transfer

**Given** an FTP directory containing nested directories and files,

**when** the operator downloads or uploads the directory,

**then** the complete hierarchy and file content are reproduced at the target.

This scenario is mandatory for every change that touches directory transfer.

## AC-007: Transfer conflict decision

**Given** a transfer whose target path already exists,

**when** the conflict is reached,

**then** the operator can overwrite, skip, or rename the target and can apply
that decision to all remaining conflicts in the operation.

## AC-008: Transfer retry and resume

**Given** a resumable SFTP or FTP transfer is interrupted after partial data is
written,

**when** automatic or manual Retry succeeds,

**then** the transfer continues from the supported offset and finishes with the
expected content.

## AC-009: Safe remote edit

**Given** a remote file visible in the file pane,

**when** the operator opens it, edits the temporary copy, and saves it,

**then** the change is uploaded and a success notification appears.

If upload or promotion fails, the prior remote file must remain intact and an
error must be shown.

## AC-010: S3 account-root browsing

**Given** an S3 connection without a fixed bucket,

**when** the operator connects,

**then** buckets appear as root folders and creating a root folder creates a
bucket.

## AC-011: S3 ACL and public URL

**Given** an S3 object on a provider with ACL support,

**when** the operator makes it public and chooses Copy public URL,

**then** the object shows a public state and the clipboard receives a URL using
the custom public base URL when configured.

## AC-012: Ask-before-upload ACL

**Given** an S3 connection whose upload ACL mode is Ask,

**when** the operator starts a batch upload,

**then** one prompt offers private, public, and cancel choices for the batch and
the saved Ask preference remains unchanged.

## AC-013: Local SSH tunnel

**Given** a connected SSH tab with a configured tunnel,

**when** the operator starts it,

**then** the declared localhost port forwards to the remote destination and the
UI shows runtime state, open connections, and traffic counters.

Stopping the tunnel must release the listener.

## AC-014: Auto-lock with a live session

**Given** an active SSH session and an inactivity timeout,

**when** the timeout expires,

**then** vault-backed UI data is locked while the network session remains
alive, and the operator can return to it after unlocking.

## AC-015: Secret-free export and idempotent import

**Given** a vault containing connections and secrets,

**when** the operator exports configuration,

**then** the JSON contains catalog/configuration data but no saved secret
values.

When the same exported document is imported repeatedly, it must merge without
duplicating the same stable catalog nodes.

## AC-016: Session cleanup

**Given** a tab with terminals, transfers, remote edits, and tunnels,

**when** the operator closes the tab,

**then** all runtime resources owned by that tab are stopped and its transfer
history is removed without affecting other tabs.

## AC-017: Cross-platform shortcuts

**Given** the same user operation on macOS and a non-macOS platform,

**when** the operator invokes a documented application shortcut,

**then** the macOS Command form and the non-macOS Control form perform the same
logical action,

**and** a selected file exposes the same visible actions menu through
`Shift+F10` and a native right-click.

## AC-018: System-aware appearance

**Given** an unlocked vault and the Appearance setting,

**when** the operator selects Light, Dark, or System,

**then** the application applies the resolved palette immediately, persists
the preference through Save, and keeps controls and saved badge colors
discernible against the active surfaces.

System mode must continue following operating-system appearance changes without
an application restart or a theme-transition animation.
