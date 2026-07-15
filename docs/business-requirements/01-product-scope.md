# Product Scope

## Purpose

Serverus is a local desktop connection manager that combines remote terminal,
dual-pane file management, file transfer, remote editing, and SSH tunnels in
one window. It is intended to replace workflows that require separate terminal
and file-transfer applications for the same server.

## Business goals

| ID | Goal |
| --- | --- |
| BR-GOAL-001 | The product must let an operator manage SSH/SFTP, FTP/FTPS, and S3-compatible resources from one application. |
| BR-GOAL-002 | A server connection must provide one consistent context for its terminal, remote files, transfers, and tunnels. |
| BR-GOAL-003 | Connections, secrets, settings, and host-key decisions must be portable as one encrypted vault file. |
| BR-GOAL-004 | Recursive directory upload and download, including FTP directories, must work reliably. |
| BR-GOAL-005 | The file-management experience must remain substantially protocol-neutral even when protocol capabilities differ. |
| BR-GOAL-006 | Routine operations must be available without requiring command-line configuration or an external transfer client. |
| BR-GOAL-007 | The product must remain usable as a personal, local-first tool without a Serverus account or hosted backend. |

## Users and roles

The product has one application role: the local operator. Typical operators
are developers, system administrators, DevOps engineers, and infrastructure
owners.

Serverus does not implement application accounts, organizations, teams,
role-based access control, or shared workspaces. Access is determined by the
ability to unlock the selected local vault.

## External actors

- an SSH/SFTP server;
- an FTP or explicit-FTPS server;
- an S3-compatible service;
- the local file system;
- the operating-system keychain and biometric service;
- the system default editor or a user-selected editor;
- the operating-system file manager.

## Core business entities

| Entity | Meaning |
| --- | --- |
| Vault | One encrypted `*.serverus` file containing the application state |
| Folder | A catalog node used to organize folders and connections |
| Connection | A saved SSH/SFTP, FTP/FTPS, or S3 configuration |
| Tab | An independent opened instance of a saved connection |
| Session | The active protocol connection owned by a tab |
| Terminal | An SSH shell channel; a session may contain several |
| File pane | The local or remote side of the dual-pane file manager |
| Transfer | One upload or download operation and its progress state |
| Tunnel | A local SSH port-forwarding rule and its runtime state |
| Known host | A previously accepted SSH server host key |

## Supported operating context

- macOS 12 and later is the primary supported environment.
- Windows and Linux builds are available but experimental.
- Touch ID is supported on macOS.
- Windows Hello is supported on Windows.
- Linux uses master-password unlock only.
- The product is distributed as a native desktop application and does not
  require a Serverus cloud service.

## In-scope capability summary

- encrypted vault creation, unlock, lock, backup-safe persistence, and move;
- nested connection catalog with badges, search, duplicate, and drag reorder;
- SSH/SFTP, FTP/explicit FTPS, and S3-compatible connections;
- multiple independent tabs and multiple terminals per SSH tab;
- dual-pane local/remote file management;
- recursive upload and download with a per-session transfer queue;
- S3 bucket, prefix, object ACL, multipart upload, and public-URL operations;
- remote-file editing through a local application;
- local SSH port forwarding;
- automatic lock and optional biometric quick unlock;
- portable, secret-free configuration export and merge-based JSON import.
