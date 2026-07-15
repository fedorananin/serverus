# Serverus Business Requirements

## Document status

- Product: Serverus
- Baseline: v1.1.0
- Document type: AS-IS business requirements
- Audience: product owners, developers, testers, security reviewers, and contributors
- Scope: user-visible behavior and the business rules that constrain it

These documents describe behavior that is already implemented. In this set,
"must" describes the current product contract, not an unapproved future
request. Known gaps and deliberately unsupported scenarios are listed
separately in [Current Limitations](10-current-limitations.md).

## Document map

| Document | Contents |
| --- | --- |
| [Product Scope](01-product-scope.md) | Product purpose, goals, actors, entities, and supported operating context |
| [Vault and Connection Catalog](02-vault-and-connection-catalog.md) | Vault lifecycle, unlock, import/export, known hosts, folders, and saved connections |
| [Protocol Connections](03-protocol-connections.md) | SSH/SFTP, FTP/FTPS, and S3 connection configuration |
| [Sessions and Terminal](04-sessions-and-terminal.md) | Tabs, reconnect, host-key verification, terminal channels, and shortcuts |
| [File Management](05-file-management.md) | Dual-pane navigation, selection, file operations, permissions, and drag-and-drop |
| [Transfers and Remote Edit](06-transfers-and-remote-edit.md) | Transfer queue, conflict handling, retry/resume, directory transfer, and remote editing |
| [S3, Tunnels, and Settings](07-s3-tunnels-and-settings.md) | S3-specific behavior, SSH tunnels, and user-configurable preferences |
| [Business Rules and Quality](08-business-rules-and-quality.md) | Security, integrity, reliability, performance, and compatibility requirements |
| [Acceptance Criteria](09-acceptance-criteria.md) | End-to-end acceptance scenarios for the implemented product |
| [Current Limitations](10-current-limitations.md) | Explicitly unsupported functionality and platform constraints |

## Requirement identifiers

Identifiers are stable within this baseline and are grouped by area:

- `BR-*`: product goals and cross-cutting business rules;
- `FR-VLT-*`: vault and access;
- `FR-CAT-*`: connection catalog;
- `FR-SSH-*`, `FR-FTP-*`, `FR-S3C-*`: protocol configuration;
- `FR-SES-*`, `FR-TRM-*`: sessions and terminal;
- `FR-FM-*`: file management;
- `FR-TRF-*`, `FR-EDT-*`: transfers and remote edit;
- `FR-S3F-*`, `FR-TUN-*`, `FR-SET-*`: S3 files, tunnels, and settings;
- `NFR-*`: non-functional requirements;
- `AC-*`: acceptance scenarios.

## Sources of truth

The behavior contract is maintained together with:

- the project [README](../../README.md);
- these business requirements;
- the generated command contract and application code;
- automated tests, especially the real SSH/SFTP, FTP, and S3 integration tests.

When documents and executable behavior disagree, the mismatch must be resolved
explicitly rather than silently treating aspirational text as implemented
functionality.
