# File Management

## Dual-pane workspace

| ID | Requirement |
| --- | --- |
| FR-FM-001 | The Files view must show the local file system on the left and the active remote file system on the right. |
| FR-FM-002 | Each pane must support entering a directory, navigating to its parent, refreshing, and editing the current path directly. |
| FR-FM-003 | Each pane must support a live filter over its current listing. |
| FR-FM-004 | Hidden files must be independently showable or hideable from the pane UI, with a global default in Settings. |
| FR-FM-005 | Listings must support sorting by applicable columns, including name, size, modified time, permissions, and S3 ACL mode. |
| FR-FM-006 | Directories must be ordered before non-directory entries. |
| FR-FM-007 | Listings with at least 10,000 entries must remain usable through virtualized rendering. |
| FR-FM-008 | File sizes must be displayable using either decimal KB units or binary KiB units. |

## Selection and navigation

| ID | Requirement |
| --- | --- |
| FR-FM-009 | A click must select one entry; `Cmd/Ctrl` must toggle entries; `Shift` must select a range. |
| FR-FM-010 | `Cmd/Ctrl+A` must select all listed entries. |
| FR-FM-011 | The operator must be able to select entries with a rubber-band rectangle and extend an existing selection with a platform modifier. |
| FR-FM-012 | Rubber-band selection must auto-scroll when the pointer reaches a pane boundary. |
| FR-FM-013 | Double-clicking a directory must enter it. Double-clicking a remote file must start remote editing. |
| FR-FM-014 | Symbolic links must be visibly distinguishable. A directory symlink may be entered, but recursive operations must not follow directory symlinks. |

## File operations

| ID | Requirement |
| --- | --- |
| FR-FM-015 | The operator must be able to create a directory in either pane. |
| FR-FM-016 | The operator must be able to create an empty file in either pane where the underlying file system permits it. |
| FR-FM-017 | The operator must be able to rename one selected entry. |
| FR-FM-018 | The operator must be able to delete one or several selected entries. Deletion must require confirmation. |
| FR-FM-019 | Deleting a selected directory must recursively remove its contents without following directory symlinks. |
| FR-FM-020 | The operator must be able to copy the path of a selected entry to the clipboard. |
| FR-FM-021 | Local selections must expose upload, and remote selections must expose download. |
| FR-FM-022 | Remote non-directory entries must expose Edit. S3 entries must additionally expose applicable ACL and public-URL actions. |
| FR-FM-023 | File operations must report actionable failures in the pane or related operation UI. |

## Permissions

| ID | Requirement |
| --- | --- |
| FR-FM-024 | For a single applicable local Unix, SFTP, or FTP entry, the operator must be able to edit owner, group, and other read/write/execute permissions. |
| FR-FM-025 | The permission dialog must keep the nine permission switches and the octal mode synchronized. |
| FR-FM-026 | For a directory, the operator may apply a permission change recursively to files, directories, or both. |
| FR-FM-027 | Local permission editing must be hidden on Windows. S3 must use object ACL operations rather than Unix permissions. |

## Keyboard operations

| ID | Requirement |
| --- | --- |
| FR-FM-028 | `F2` or `Enter` must rename the single selected entry. |
| FR-FM-029 | `Backspace` must navigate to the parent directory. |
| FR-FM-030 | `Delete` or `Cmd/Ctrl+Backspace` must initiate deletion of the current selection. |
| FR-FM-031 | `Cmd/Ctrl+Right` must upload the selected local entries. |
| FR-FM-032 | `Cmd/Ctrl+Left` must download the selected remote entries. |

## Drag-and-drop

| ID | Requirement |
| --- | --- |
| FR-FM-033 | Local entries must be draggable out of Serverus to the operating-system file manager through native drag-out. |
| FR-FM-034 | Files dropped from the operating system onto the local pane must be copied recursively into the current local directory. |
| FR-FM-035 | Files dropped from the operating system onto the remote pane must be queued for upload. |
| FR-FM-036 | Remote entries dragged onto the local pane must be queued for download. |
| FR-FM-037 | In-app remote drag-and-drop must use pointer-driven behavior rather than browser HTML5 drag data. |
