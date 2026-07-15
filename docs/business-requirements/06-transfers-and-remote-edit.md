# Transfers and Remote Edit

## Transfer queue

| ID | Requirement |
| --- | --- |
| FR-TRF-001 | The operator must be able to upload local files and directories and download remote files and directories. |
| FR-TRF-002 | Directory transfers must be recursive for SFTP, FTP/FTPS, and S3. |
| FR-TRF-003 | Transfer queue and history must be isolated to the owning session and cleared when its tab closes or the application exits. |
| FR-TRF-004 | The product must run several file transfers in parallel. The configured limit must be between 1 and 16, with 5 as the default. |
| FR-TRF-005 | The transfer panel must auto-expand for active work and remain manually collapsible. |
| FR-TRF-006 | Each item must show direction, name, progress, completed and total bytes, speed, ETA, state, and any error. |
| FR-TRF-007 | Supported states must include queued, running, paused, conflict, done, skipped, cancelled, and error. |
| FR-TRF-008 | Each applicable item must offer Pause, Resume, Cancel, and Retry. |
| FR-TRF-009 | The queue must offer Pause all, Resume all, Cancel all, and Clear finished actions. |

## Conflicts and retries

| ID | Requirement |
| --- | --- |
| FR-TRF-010 | When a target already exists, the product must support Overwrite, Skip, and Rename decisions. |
| FR-TRF-011 | The operator must be able to apply a conflict decision to all remaining conflicts in the current operation. |
| FR-TRF-012 | Settings must support ask, overwrite, skip, and rename as the default conflict policy. |
| FR-TRF-013 | A failed transfer must receive up to two automatic retries before remaining in an error state. |
| FR-TRF-014 | After automatic retries are exhausted, the operator must be able to retry manually. |
| FR-TRF-015 | A cancelled ordinary transfer must remove the partial target instead of presenting it as a completed file. |

## Resume and metadata

| ID | Requirement |
| --- | --- |
| FR-TRF-016 | SFTP retry must resume from the supported byte offset. |
| FR-TRF-017 | FTP retry must use REST resume when the server supports it. |
| FR-TRF-018 | S3 range downloads may resume, while S3 uploads must restart. |
| FR-TRF-019 | The operator may enable preservation of source modification time where the protocol supports it. S3 object modification time is not settable by the client. |

## Directory acceleration

| ID | Requirement |
| --- | --- |
| FR-TRF-020 | For an SSH/SFTP directory, the product may use one tar stream when a compatible remote `tar` command is available and acceleration is enabled. |
| FR-TRF-021 | A tar-accelerated item must be visibly identified as `via tar`. |
| FR-TRF-022 | If a tar transfer fails, a manual Retry must fall back to the ordinary per-file directory queue. |
| FR-TRF-023 | FTP and S3 directory transfers must remain recursive without depending on tar. |

## Remote edit

| ID | Requirement |
| --- | --- |
| FR-EDT-001 | Double-clicking or selecting Edit on a remote file must download an isolated temporary copy and open it in the configured local application. |
| FR-EDT-002 | The operator must be able to use the system default application or configure a custom editor application. |
| FR-EDT-003 | The product must watch the temporary file and upload changes after each local save, with debounce against duplicate file-system events. |
| FR-EDT-004 | The UI must notify the operator when an edited file is uploaded successfully or when synchronization fails. |
| FR-EDT-005 | Remote replacement must use a safe staging flow so that a failed upload or promotion does not destroy the previous remote file. |
| FR-EDT-006 | Remote edit must work through the active remote file-system abstraction for SFTP, FTP/FTPS, and S3. |
| FR-EDT-007 | Temporary edit files and watchers must be removed when the owning session closes or the application exits. |
