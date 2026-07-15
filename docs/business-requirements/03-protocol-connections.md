# Protocol Connections

## Common connection behavior

| ID | Requirement |
| --- | --- |
| FR-CON-001 | Every connection must have a name and protocol-specific endpoint configuration. |
| FR-CON-002 | A connection may store a badge, initial local directory, initial remote location, and operator notes. |
| FR-CON-003 | Secret values must be redacted from the normal public vault model sent to the frontend. |
| FR-CON-004 | While the vault is unlocked, editing a connection must retrieve its stored secret values on demand so that the operator can review or replace them. |
| FR-CON-005 | The protocol of an existing connection must not be changeable in the edit dialog. The operator may create or duplicate another connection instead. |

## SSH and SFTP configuration

| ID | Requirement |
| --- | --- |
| FR-SSH-001 | The operator must be able to configure connection name, host, port, username, starting local directory, starting remote directory, badge, and notes. |
| FR-SSH-002 | The product must support password authentication. |
| FR-SSH-003 | The product must support private-key authentication using either a local key-file path or key text stored inside the encrypted vault. |
| FR-SSH-004 | The operator must be able to choose a key file through a native file picker. |
| FR-SSH-005 | The operator must be able to import a private key file as encrypted vault text after the file has passed basic private-key validation. |
| FR-SSH-006 | The operator must be able to supply a passphrase for an encrypted private key. |
| FR-SSH-007 | The product must support SSH Agent authentication. |
| FR-SSH-008 | The operator must be able to mark a connection as SFTP-only for accounts that do not provide a shell. |
| FR-SSH-009 | An SFTP-only connection must not open a shell channel and must not expose the Terminal view. |
| FR-SSH-010 | The operator must be able to select another saved SSH connection as a jump host. |
| FR-SSH-011 | Jump-host chains must be supported up to the implementation safety limit, and cycles must be rejected. |
| FR-SSH-012 | The operator must be able to define zero or more local tunnel rules as part of an SSH connection. |

## FTP and explicit FTPS configuration

| ID | Requirement |
| --- | --- |
| FR-FTP-001 | The operator must be able to configure connection name, host, port, username, password, starting local directory, starting remote directory, badge, and notes. |
| FR-FTP-002 | The operator must be able to select passive or active FTP mode. |
| FR-FTP-003 | The operator must be able to require explicit FTPS through `AUTH TLS`. |
| FR-FTP-004 | If explicit FTPS is required and TLS cannot be established, the connection must fail rather than fall back to plaintext FTP. |
| FR-FTP-005 | FTP and FTPS sessions must use the same remote file-management and transfer model as other remote file systems. |

## S3-compatible configuration

| ID | Requirement |
| --- | --- |
| FR-S3C-001 | The operator must be able to configure endpoint, port, access key ID, secret access key, region, badge, notes, and starting local location. |
| FR-S3C-002 | The product must provide presets for DigitalOcean Spaces, AWS S3, Cloudflare R2, Backblaze B2, Wasabi, and MinIO/custom endpoints. |
| FR-S3C-003 | A preset may prefill fields but must leave the resulting configuration editable. |
| FR-S3C-004 | The operator may bind a connection to one bucket or leave the bucket empty to browse the account bucket root. |
| FR-S3C-005 | The operator must be able to enable path-style addressing for MinIO and other compatible custom endpoints. |
| FR-S3C-006 | The operator may configure a public base URL or CDN domain used when copying public object URLs. |
| FR-S3C-007 | The operator must be able to choose the default upload ACL mode: private, public-read, or ask for each upload batch. |
