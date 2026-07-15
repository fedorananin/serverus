# Serverus Context Map

This map fixes Serverus's domain language and the ownership boundaries used by
the target architecture. The workspace crates and the first Transfers/runtime
slices now exist; the same boundaries govern the remaining incremental
migration. Detailed rules live in [ARCHITECTURE.md](ARCHITECTURE.md).

## Language

**Vault**:
The encrypted Serverus document containing connection definitions, secrets,
the catalog tree, known hosts, and settings.
_Avoid_: Workspace, database, config file

**Selected vault**:
The vault path currently selected by the desktop application, whether or not
that vault is unlocked.
_Avoid_: Active workspace

**Vault lock**:
Revocation of access to the selected vault's decrypted payload and secrets.
It revokes the current vault-access epoch but does not end the runtime context
or already-authenticated network sessions.
_Avoid_: Logout, disconnect

**Vault access epoch**:
The monotonic authorization generation created by each successful unlock.
Secret-consuming work from an older epoch stays stale after lock and re-unlock.
_Avoid_: Session generation, runtime context

**Vault switch**:
Replacement of the selected vault with another vault. Unlike a vault lock, a
switch retires the old runtime context and all work owned by it.
_Avoid_: Lock

**Connection**:
A persisted definition of how to reach and authenticate to an SSH, FTP, or S3
endpoint. A connection is configuration, not a live network resource.
_Avoid_: Session, server

**Session**:
A live connection instance opened from a connection definition. Two sessions
may be opened from the same connection.
_Avoid_: Connection, tab

**Runtime context**:
A generation-scoped owner of all live sessions, transfers, tunnels, terminal
channels, and remote edits associated with one selected vault.
_Avoid_: Global state, workspace

**Remote endpoint**:
A live, capability-bearing interface to remote resources. Its supported
operations are discovered from capabilities rather than inferred outside an
adapter from a protocol enum.
_Avoid_: Remote filesystem when object-store semantics matter

**Remote resource**:
A file, directory-like prefix, object, or bucket exposed by a remote endpoint.
_Avoid_: File when the item may be an object-store prefix or bucket

**Transfer**:
A runtime-owned operation that moves bytes and selected metadata between a
local resource and a remote endpoint according to a transfer plan.
_Avoid_: Copy job

**Remote edit**:
The lifecycle that downloads a remote file into a private local cache, opens
it in an editor, watches saves, and safely promotes updates remotely.
_Avoid_: Transfer

**Host-key challenge**:
A pending SSH trust decision identified by a backend-issued challenge ID.
It is an application state, not an exceptional transport failure.
_Avoid_: Host-key error, prompt error

## Contexts

- [Vault and Connection Catalog](ARCHITECTURE.md#vault-and-connection-catalog)
  owns vault lifecycle, connection definitions, the catalog tree, settings,
  imports, known-host records, and secret references.
- [Connectivity](ARCHITECTURE.md#connectivity) owns session lifecycle, SSH
  trust and authentication, jump chains, reconnect, terminal channels, and
  tunnels.
- [Remote Resources](ARCHITECTURE.md#remote-resources) owns remote browsing,
  paths, resource metadata, stream access, and endpoint capabilities.
- [Transfers](ARCHITECTURE.md#transfers) owns transfer planning, expansion,
  scheduling, conflict handling, retry, progress, and execution strategy.
- [Remote Edit](ARCHITECTURE.md#remote-edit) owns the edit cache, watcher
  lifecycle, safe upload, promotion, rollback, and cleanup workflow.
- [Desktop Platform](ARCHITECTURE.md#desktop-platform) is a supporting
  boundary for biometrics, sleep/activity signals, native file operations,
  editor launching, and drag-and-drop.

## Relationships

- **Vault and Connection Catalog -> Connectivity**: supplies a redacted
  connection definition plus secrets resolved only for an authorized use case.
- **Connectivity -> Remote Resources**: exposes a remote endpoint handle and
  its capabilities for a live session.
- **Transfers -> Remote Resources**: builds and executes plans against
  capability ports; it does not select SFTP, FTP, or S3 implementations.
- **Remote Edit -> Remote Resources**: reads, stages, promotes, and cleans up
  remote content through endpoint capabilities.
- **Remote Edit -> Desktop Platform**: launches an editor and consumes file
  watcher events through ports.
- **All runtime contexts -> Vault and Connection Catalog**: carry the selected
  vault identity and generation, but never persist runtime handles in a vault.
- **Desktop shell -> all application contexts**: invokes use cases and maps
  typed application events to Tauri IPC; contexts never depend on Tauri.

No context communicates through Tauri events internally. Application events
and snapshot queries form the internal contract; the desktop shell is only an
adapter for that contract.
