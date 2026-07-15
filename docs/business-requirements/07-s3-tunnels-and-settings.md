# S3, Tunnels, and Settings

## S3 file behavior

| ID | Requirement |
| --- | --- |
| FR-S3F-001 | When no fixed bucket is configured, the remote root must list buckets as folders. |
| FR-S3F-002 | Creating a folder at the account root must create a bucket. Deleting a bucket must first recursively remove its contents. |
| FR-S3F-003 | Inside a bucket, object prefixes must be represented as navigable folders. |
| FR-S3F-004 | S3 listings must expose an ACL mode column whose values may be public, private, loading, unknown, or error. |
| FR-S3F-005 | ACL badges may load asynchronously after the base file listing is shown. |
| FR-S3F-006 | The operator must be able to make selected objects public or private. Folder ACL changes must apply recursively. |
| FR-S3F-007 | The operator must be able to apply public/private ACL changes to multiple selected entries. |
| FR-S3F-008 | The operator must be able to copy the public URL of a file. The configured public base URL must take precedence over the endpoint-derived URL. |
| FR-S3F-009 | In ask mode, one dialog per upload batch must allow private upload, public upload, or cancellation. The runtime choice must not replace the saved ask preference. |
| FR-S3F-010 | Providers that do not expose compatible ACL behavior must produce an unknown state or error rather than a false public/private result. |
| FR-S3F-011 | Large object uploads must use multipart upload. An incomplete multipart upload must be aborted after cancellation or failure. |

## Local SSH tunnels

| ID | Requirement |
| --- | --- |
| FR-TUN-001 | An SSH connection may define multiple local port-forwarding rules. |
| FR-TUN-002 | Each rule must contain a name, local port, remote host, remote port, and autostart preference. |
| FR-TUN-003 | The Tunnels view must be available only when the active SSH connection contains configured tunnels. |
| FR-TUN-004 | The operator must be able to start and stop each configured tunnel. |
| FR-TUN-005 | A tunnel must display its `localhost:local-port -> remote-host:remote-port` mapping and current runtime state. |
| FR-TUN-006 | A running tunnel must display open connection count and uploaded/downloaded byte counters. |
| FR-TUN-007 | Tunnel listeners must bind to localhost rather than expose the local port on all network interfaces. |
| FR-TUN-008 | A rule marked autostart must start when its owning SSH session becomes available. |

## Settings

| ID | Requirement |
| --- | --- |
| FR-SET-001 | Security settings must include inactivity timeout from 0 to 1,440 minutes, lock on sleep, and quick-unlock enablement when supported. |
| FR-SET-002 | Transfer settings must include parallel transfer limit, default conflict policy, modification-time preservation, and tar acceleration. |
| FR-SET-003 | Editor settings must allow the system default application or a custom application. |
| FR-SET-004 | Terminal settings must include font family, font size from 8 to 32, scrollback from 100 to 100,000 lines, and copy on select. |
| FR-SET-005 | File-panel settings must include hidden-file visibility, KB/KiB size units, and a default local directory. |
| FR-SET-006 | Vault settings must expose its active path, move, master-password change, export, and import actions. |
| FR-SET-007 | Known-host settings must expose saved host records and an individual Forget action. |
| FR-SET-008 | About must show the application version, MIT license status, and a GitHub repository action that opens externally. |
| FR-SET-009 | Ordinary preference edits must be committed through Save and discarded through Cancel. Immediate vault-management and known-host actions may report their own result independently. |
| FR-SET-010 | Appearance must offer System, Light, and Dark choices. A choice must preview immediately; Save persists it, while Cancel restores the saved preference. |
