# Vault and Connection Catalog

All requirements in this document describe implemented v1.1.0 behavior.

## Vault lifecycle and access

| ID | Requirement |
| --- | --- |
| FR-VLT-001 | On first use, the operator must be able to choose a `*.serverus` location, enter and confirm a master password, and create an encrypted vault. |
| FR-VLT-002 | The product must warn that the master password cannot be recovered. The password must never be persisted. |
| FR-VLT-003 | The operator must be able to select and unlock an existing vault with its master password. |
| FR-VLT-004 | After a successful password unlock, the operator may enable quick unlock through Touch ID on macOS or Windows Hello on Windows. |
| FR-VLT-005 | The operator must be able to disable an enabled quick-unlock mechanism. Linux must remain password-only. |
| FR-VLT-006 | The operator must be able to lock the application manually. Locking must remove vault data from the active UI without terminating already running network sessions. |
| FR-VLT-007 | The product must support automatic lock after a configurable inactivity interval. A value of zero must disable inactivity lock. |
| FR-VLT-008 | The operator must be able to enable lock-on-system-sleep behavior. |
| FR-VLT-009 | The lock screen must allow the operator to select another existing vault or choose a path for a new vault without first unlocking the current vault. |
| FR-VLT-010 | The operator must be able to change the master password by supplying the current password and confirming the replacement password. |
| FR-VLT-011 | The operator must be able to move the active vault to a typed path or a path selected in a native save dialog. The old file must remain at the previous path as a manual backup. |
| FR-VLT-012 | The product must refuse to overwrite an existing file when moving the vault. |

## Configuration exchange

| ID | Requirement |
| --- | --- |
| FR-VLT-013 | The operator must be able to export the catalog and settings as unencrypted JSON. |
| FR-VLT-014 | Export must omit passwords, key passphrases, embedded private-key text, and S3 secret access keys. |
| FR-VLT-015 | The UI must warn that the exported JSON itself is unencrypted even though secrets are omitted. |
| FR-VLT-016 | The operator must be able to import a Serverus export or a hand-written document conforming to `docs/CONFIG_FORMAT.md`. |
| FR-VLT-017 | Import must merge into the current vault rather than replace it wholesale. Imported nodes with stable identities must replace their previous instances so that re-import does not duplicate them. |
| FR-VLT-018 | An imported document may contain plaintext secrets; after import, those values must be stored inside the encrypted vault. |
| FR-VLT-019 | Existing known-host entries must win over conflicting imported entries because the local operator has already verified them. |
| FR-VLT-020 | The UI must report the number of imported connections or an actionable import error. |

## Known hosts

| ID | Requirement |
| --- | --- |
| FR-VLT-021 | The settings UI must list saved SSH host-key records. |
| FR-VLT-022 | The operator must be able to forget an individual known-host record. The next connection to that host must require host-key confirmation again. |

## Connection catalog

| ID | Requirement |
| --- | --- |
| FR-CAT-001 | Saved connections must be presented in a hierarchical sidebar tree. |
| FR-CAT-002 | The operator must be able to create folders at the root or inside other folders. |
| FR-CAT-003 | The operator must be able to rename a folder, edit its badge, collapse it, and expand it. |
| FR-CAT-004 | A collapsed folder must show its contained item count; an expanded folder must not show that count. |
| FR-CAT-005 | Deleting a folder must require confirmation and promote its children one level instead of deleting the children. |
| FR-CAT-006 | The operator must be able to create a connection at the root or in a selected folder. |
| FR-CAT-007 | The operator must be able to edit, duplicate, and delete a connection. Deletion must require confirmation. |
| FR-CAT-008 | A duplicated connection must receive a new identity and a copy-suffixed name while retaining the copied configuration. |
| FR-CAT-009 | Folders and connections must support no badge, a preset color, an emoji, or a custom hexadecimal color. |
| FR-CAT-010 | The operator must be able to reorder catalog nodes before or after another node, move them into a folder, or return them to the root. |
| FR-CAT-011 | The product must reject attempts to move a folder into itself or one of its descendants. |
| FR-CAT-012 | Sidebar search must match connection name or host and present matching connections as a flat sorted result set. |
| FR-CAT-013 | Double-click or Enter on a connection must open a new tab and start a connection attempt. |
| FR-CAT-014 | Connection and folder actions must also be available from their context menus where applicable. |
