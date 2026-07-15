# Sessions and Terminal

## Tabs and session lifecycle

| ID | Requirement |
| --- | --- |
| FR-SES-001 | Opening a saved connection must create a new tab with an independent network session. |
| FR-SES-002 | The same saved connection may be opened in several tabs at the same time. Closing one tab must not close another instance. |
| FR-SES-003 | A regular SSH connection must open the Terminal view by default. FTP, S3, and SFTP-only SSH connections must open the Files view. |
| FR-SES-004 | Files must be available for every connected protocol. Terminal must be available only for shell-enabled SSH. Tunnels must be available only for SSH connections that define tunnels. |
| FR-SES-005 | A tab must communicate connecting, connected, disconnected, and error states. |
| FR-SES-006 | During connection setup, the UI must show meaningful stage messages. A long-running attempt must additionally show elapsed time. |
| FR-SES-007 | A failed or disconnected tab must provide a Retry action. |
| FR-SES-008 | An unexpectedly dropped SSH session must make up to three automatic reconnect attempts with increasing delays. |
| FR-SES-009 | After successful SSH reconnect, the product must attempt to restore the last remote directory. |
| FR-SES-010 | Closing a tab must disconnect its session, close its terminal channels, stop its tunnels and remote-edit watchers, and clear its transfer queue and history. |
| FR-SES-011 | Horizontal tab overflow must remain navigable. Middle-click must close the targeted tab. |

## Tab and application shortcuts

| ID | Requirement |
| --- | --- |
| FR-SES-012 | `Cmd+T` on macOS or `Ctrl+T` elsewhere must open another tab for the active saved connection. |
| FR-SES-013 | `Cmd+W` on macOS or `Ctrl+W` elsewhere must close the active tab. |
| FR-SES-014 | `Cmd/Ctrl+1` through `Cmd/Ctrl+9` must activate the corresponding tab when it exists. |
| FR-SES-015 | `Cmd+,` on macOS or the platform-equivalent `Ctrl+,` shortcut must open Settings. |

## SSH host-key verification

| ID | Requirement |
| --- | --- |
| FR-SES-016 | When an SSH host key is unknown, the UI must display the host, key algorithm, and fingerprint before authentication continues. |
| FR-SES-017 | The operator must be able to accept and save the key or reject the connection. |
| FR-SES-018 | A host key that differs from a previously saved value must produce a distinct high-severity warning. |
| FR-SES-019 | Rejecting a host key must stop the connection and surface an error state. |

## Terminal channels

| ID | Requirement |
| --- | --- |
| FR-TRM-001 | One SSH tab must support multiple independent terminal channels. |
| FR-TRM-002 | The operator must be able to add, select, and close individual terminal channels without closing the SSH tab. |
| FR-TRM-003 | Each terminal must use an interactive PTY compatible with `xterm-256color`, 256-color applications, and true color. |
| FR-TRM-004 | Terminal dimensions must follow the visible container size and propagate resize changes to the remote PTY. |
| FR-TRM-005 | Switching to Files or Tunnels must hide rather than destroy terminal views, so shells continue running in the background. |
| FR-TRM-006 | A terminal whose remote shell exits must remain visible with a clear `Shell exited` state. |
| FR-TRM-007 | Recognized web links in terminal output must be clickable. |

## Terminal interaction

| ID | Requirement |
| --- | --- |
| FR-TRM-008 | The operator must be able to search terminal history, move to the next or previous match, and close the search UI. |
| FR-TRM-009 | Terminal search must use `Cmd+F` on macOS and `Ctrl+Shift+F` on other platforms. |
| FR-TRM-010 | Selected text must be copyable with `Cmd+C` on macOS and `Ctrl+Shift+C` on other platforms. |
| FR-TRM-011 | The operator may enable automatic copy when terminal text is selected. |
| FR-TRM-012 | Single-line clipboard text may be pasted immediately. |
| FR-TRM-013 | Multi-line clipboard text must require confirmation showing the line count and a preview before it is sent to the remote shell. |
| FR-TRM-014 | The operator must be able to configure terminal font family, font size, and scrollback capacity. |
