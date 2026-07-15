# Business Rules and Quality Requirements

## Cross-cutting business rules

| ID | Rule |
| --- | --- |
| BR-SEC-001 | Passwords, key passphrases, private keys, S3 secrets, and the decrypted data-encryption key must never appear in logs, debug output, ordinary error messages, or broader Tauri responses than the UI operation requires. |
| BR-SEC-002 | The master password must never be persisted or placed in a recovery mechanism. |
| BR-SEC-003 | Secret buffers must be zeroized when the vault locks or the owning data is dropped. |
| BR-SEC-004 | Cryptography must use audited libraries and standard constructions; the project must not introduce hand-written cryptography. |
| BR-VLT-001 | Every vault write must be atomic and retain the previous version as a `.bak` file. |
| BR-VLT-002 | The decrypted vault payload must never be written to disk. |
| BR-PRO-001 | User-facing remote file operations and the transfer queue must remain independent of whether the active implementation is SFTP, FTP, or S3. |
| BR-PRO-002 | Recursive FTP directory transfer is a founding product invariant and must remain covered by integration tests. |
| BR-SES-001 | Runtime terminals, transfers, tunnels, and remote edits belong to one session and must not leak into another tab. |
| BR-UI-001 | Destructive deletion and multi-line shell paste must require explicit confirmation. |
| BR-UI-002 | Components must use the shared platform shortcut mapping; they must not implement their own direct `metaKey` assumptions. |
| BR-OS-001 | Operating-system-specific behavior must remain isolated and degrade safely where a capability is unavailable. |

## Security requirements

| ID | Requirement |
| --- | --- |
| NFR-SEC-001 | Vault key derivation must use Argon2id and vault encryption must use AES-256-GCM with a random data-encryption key. |
| NFR-SEC-002 | Quick unlock must protect access to the data-encryption key with the platform security facility rather than storing the master password. |
| NFR-SEC-003 | SSH sessions must verify unknown and changed host keys before proceeding. |
| NFR-SEC-004 | Explicit FTPS must never silently downgrade to plaintext FTP. |
| NFR-SEC-005 | Local tunnel listeners must default to the loopback interface only. |
| NFR-SEC-006 | Secret-free public connection models must be the default frontend representation. |

## Integrity and reliability

| ID | Requirement |
| --- | --- |
| NFR-REL-001 | A failed vault save must leave either the old valid vault or the new valid vault, never a partially written primary file. |
| NFR-REL-002 | Remote edit must preserve the old remote file when upload or final replacement fails. |
| NFR-REL-003 | Transfer cancellation and S3 multipart failure must clean up protocol-specific partial state where supported. |
| NFR-REL-004 | Network interruptions must produce bounded retries and visible errors rather than indefinite silent work. |
| NFR-REL-005 | Session cleanup must release network channels, watchers, tunnel listeners, and transfer state. |
| NFR-REL-006 | Directory recursion must not follow directory symbolic links. |

## Performance and responsiveness

| ID | Requirement |
| --- | --- |
| NFR-PERF-001 | File panes must remain interactive for listings of 10,000 or more entries. |
| NFR-PERF-002 | Independent transfers must run concurrently up to the configured limit. |
| NFR-PERF-003 | SSH/SFTP directory transfer should use tar streaming when safely available and enabled. |
| NFR-PERF-004 | Large S3 uploads must use multipart transfer. |
| NFR-PERF-005 | S3 ACL lookup must not block display of the base object listing. |
| NFR-PERF-006 | Hidden terminal channels must stay alive without fitting themselves against a zero-sized container. |

## Usability

| ID | Requirement |
| --- | --- |
| NFR-UX-001 | Connection status, transfer status, errors, retries, and destructive confirmations must be visible and actionable. |
| NFR-UX-002 | Protocol differences must be exposed only when they change a real operator decision, such as S3 ACL versus Unix permissions. |
| NFR-UX-003 | Native pickers, system editor integration, clipboard actions, and file-manager drag-and-drop must be used where they reduce manual path entry. |
| NFR-UX-004 | Platform shortcuts must use Command conventions on macOS and Control conventions elsewhere. |

## Compatibility and verification

| ID | Requirement |
| --- | --- |
| NFR-COMP-001 | macOS 12+ must be treated as the primary supported platform. |
| NFR-COMP-002 | Windows and Linux builds must compile in CI and remain explicitly labeled experimental until exercised on real hardware. |
| NFR-COMP-003 | macOS and Linux integration tests must exercise real unprivileged SSH/SFTP plus in-process FTP and S3 servers without Docker. |
| NFR-COMP-004 | Windows CI may run unit tests without the Unix-specific integration test harness. |
| NFR-COMP-005 | Releases must be buildable for dmg, msi/nsis, AppImage, deb, and rpm through the release workflow. |
