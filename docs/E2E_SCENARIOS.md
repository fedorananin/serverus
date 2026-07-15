# Desktop scenario tests

Serverus uses WebdriverIO with the Tauri service and its embedded WebDriver
provider. A scenario controls the real application and its platform WebView
instead of a browser mock:

```text
                     ┌→ WKWebView (macOS) ──┐
WebdriverIO → driver ─┼→ WebKitGTK (Linux) ├→ Svelte → real Tauri invoke
                     └→ WebView2 (Windows) ┘          → Rust → local fixture
```

This is the current cross-platform approach recommended by the
[Tauri WebDriver guide](https://v2.tauri.app/develop/tests/webdriver/) and the
[WebdriverIO Tauri quick start](https://webdriver.io/docs/desktop-testing/tauri/quick-start/).

## Commands

```bash
npm run scenarios:check # unit contracts, TypeScript and catalog/layout integrity
npm run test:scenarios  # build fixtures + isolated Tauri app, then run the catalog
```

Select scenarios by stable ID or split the selected catalog into deterministic,
contiguous shards. Filtering happens before sharding and keeps catalog order:

```bash
E2E_SCENARIOS=vault-lifecycle,ftp-recursive-transfer npm run test:scenarios
E2E_SCENARIO_SHARD_INDEX=2 E2E_SCENARIO_SHARDS_TOTAL=3 npm run test:scenarios
```

`SERVERUS_SCENARIO_SKIP_BUILD=1` is available for a local rerun after both
scenario binaries have already been built. The runner hashes every runtime
source, lockfile and build configuration and refuses the shortcut if that
fingerprint differs from the completed build. It also validates binary paths
through `cargo metadata`; it never assumes an in-repository `target/`.
Scenario binaries live in a dedicated `scenario-tests/` child of that external
Cargo target so test-only features never replace the normal debug executable.

Scenario builds default to `CARGO_BUILD_JOBS=1`. This keeps concurrent Rust
compiler memory use predictable on smaller CI and developer machines. An
existing `CARGO_BUILD_JOBS` value takes precedence; otherwise set
`SERVERUS_SCENARIO_BUILD_JOBS=<positive integer>` for a deliberate local
override.

## Catalog and acceptance ownership

| ID | Acceptance | Platforms | Input fidelity | Contract |
| --- | --- | --- | --- | --- |
| `vault-lifecycle` | AC-001 | macOS, Linux, Windows | `real-input` | Create a real encrypted vault, lock it, reject a wrong password and unlock it. |
| `ssh-terminal` | AC-003 | macOS, Linux | `real-input` | Show the unknown-host identity, accept and persist it, execute a command, then detect and reject a changed host key. |
| `session-lifecycle` | AC-005, AC-013, AC-014, AC-016 | macOS, Linux | `real-input` | Keep multiple terminals, a tunnel and a live session isolated through auto-lock and deterministic tab cleanup. |
| `ftp-tab-isolation` | AC-004 | macOS, Linux, Windows | `real-input` | Open one saved FTP connection twice, isolate both tabs' navigation state, then close one and keep using the sibling session. |
| `ftp-recursive-transfer` | AC-006 | macOS, Linux, Windows | `real-input` | Upload and download a complete nested tree through the real FTP queue and compare every file byte. |
| `transfer-resilience` | AC-007, AC-008 | macOS, Linux, Windows | `real-input` | Scope conflict decisions to one batch and resume/retry an interrupted FTP transfer. |
| `remote-edit-safety` | AC-009 | macOS, Linux, Windows | `real-input` | Edit through an external fixture process, publish a successful save, and preserve the original remote file when promotion fails. |
| `s3-buckets` | AC-010 | macOS, Linux, Windows | `real-input` | List and create buckets against an in-process S3-compatible server. |
| `s3-sharing` | AC-011, AC-012 | macOS, Linux, Windows | `real-input` | Exercise Ask/private/public upload ACLs, publish an object and copy its custom public URL. |
| `platform-shortcuts` | AC-017 | macOS, Linux, Windows | `real-input` | Exercise `T`, `W`, `1`, `2`, comma and `A` with the platform's real modifier key, then open selected-file actions through the visible **Actions** button. |

`real-input` means WebDriver clicks, types and presses keys through visible,
accessible WebView controls. This includes the SSH scenarios: WebKit WebDriver
does not reliably deliver W3C key input to xterm, so each terminal probe is
entered through the visible Terminal **Paste…** field. A multiline probe must pass
the same visible confirmation dialog as a human paste. The suite does not type
into xterm's hidden helper textarea, inject DOM events, assign DOM values or
bypass the Tauri command layer and SSH server.

Two criteria deliberately have manual-native owners:

| ID | Acceptance | Platforms | Why it is manual-native |
| --- | --- | --- | --- |
| `quick-unlock-native` | AC-002 | macOS, Windows | Touch ID and Windows Hello prompts are owned by the operating system and are outside WebDriver control. |
| `config-roundtrip-native` | AC-015 | macOS, Linux, Windows | Export/import paths are selected through native operating-system dialogs. |

Manual-native supplements record a user-input subpath that the automated owner
cannot honestly exercise. They are not acceptance owners and therefore do not
change exact ownership or turn an automated criterion into a second owner.
A `required-path` supplement makes that criterion mixed rather than fully
automated; an `additional-variant` records useful native breadth beyond the
automated acceptance contract without downgrading it:

| ID | Automated owner | Acceptance | Impact | Platforms | Manual action | Why automation does not claim it |
| --- | --- | --- | --- | --- | --- | --- |
| `platform-shortcuts-arrow-transfer-native` | `platform-shortcuts` | AC-017 | `required-path` | macOS, Linux, Windows | Select a local file and press `Cmd/Ctrl+Right`; select a remote file and press `Cmd/Ctrl+Left`; verify both transfers complete with identical bytes. | `tauri-plugin-wdio-webdriver` 1.2.0 sends Arrow keys without retaining the active modifier flags. |
| `platform-keyboard-context-menu-native` | `platform-shortcuts` | AC-017 | `required-path` | macOS, Linux, Windows | Select a visible local and remote file row, press `Shift+F10`, and verify the actions menu opens for that selection and its enabled action works. | `tauri-plugin-wdio-webdriver` 1.2.0 dispatches F10 without the held Shift modifier. |
| `platform-context-menu-native` | `platform-shortcuts` | AC-017 | `required-path` | macOS, Linux, Windows | Right-click a visible local and remote file row; verify the actions menu opens at the pointer and its enabled action works. | `tauri-plugin-wdio-webdriver` 1.2.0 emits secondary-button down/up events without a `contextmenu` event, so no embedded platform driver can prove the native right-button path. |
| `remote-edit-native-editor` | `remote-edit-safety` | AC-009 | `additional-variant` | macOS, Linux, Windows | Configure an installed editor, open a remote file, save a unique change, and verify the visible upload result and remote bytes. | WebDriver cannot control arbitrary native editor windows or OS launchers; automation uses a deterministic external editor process. |

The deterministic editor executable makes AC-009 repeatable: it is launched
as a real external process, edits the downloaded temp file, and lets the real
watcher and FTP promotion path run. It intentionally does not claim coverage
of every OS launcher or installed editor. The typed native-editor supplement
requires a release check with one concrete editor (for example Visual Studio
Code: its application name on macOS and executable path on Windows/Linux),
double-click a remote file, save it in that editor, and confirm the upload
through both the visible status and remote bytes.

The authoritative typed catalogs are `e2e-scenarios/src/scenarios.ts` and
`e2e-scenarios/src/scenario-supplements.ts`. `npm run scenarios:check` parses the
`AC-001`...`AC-017` headings in
`docs/business-requirements/09-acceptance-criteria.md` and compares them with
the typed IDs. Every acceptance criterion must have exactly one automated or
manual-native owner; missing, duplicate and unknown IDs fail the gate. Every
supplement must name that automated owner, one of its acceptance IDs and only
platforms automated by it; detached, conflicting and duplicate supplements
also fail the gate.

Every scenario ID must also have one matching directory and one
`<id>.e2e.spec.ts` entry file. The layout gate parses TypeScript syntax rather
than searching strings: it requires an exact `describe("@<id>", ...)` and at
least one executable `it`/`test` inside that suite. A comment, an empty tagged
suite, a detached test, an unregistered directory or an extra nested/root spec
that WDIO would never select cannot make the check pass.

## Isolation and security

Scenario support is enabled only by the explicit Rust `scenario-tests` feature
and the Vite `scenarios` mode. That build:

- requires `SERVERUS_SCENARIO_CONFIG_DIR` and fails closed when it is absent;
- uses a disposable config directory and vault files owned by the fixture
  process;
- replaces Keychain, Touch ID and Windows Hello with `NoQuickUnlock`;
- registers `tauri-plugin-wdio` and `tauri-plugin-wdio-webdriver` and grants
  their permissions through a test-only inline capability;
- enables the frontend WDIO bridge only in scenario mode.

Normal debug and release builds contain none of that command surface. Never
weaken this boundary to `debug_assertions`: ordinary development builds may use
the real user config and OS credential store.

Fixture stdout is a single readiness JSON document containing temporary paths,
loopback endpoints and the deterministic editor executable. Passwords, private
key contents, access keys and secret keys are forbidden in that manifest. The
runner validates this before starting the application and disables WDIO
frontend/backend log capture. The top-level WDIO command logger is `silent`:
pinned WebDriver errors can contain the original input request body, so even
error-level command logging is forbidden. Test-only S3 credentials are fixed
constants used only by the disposable local server.

Fresh disposable vault selection also goes through the visible `Vault path`
field and `Use path` button. File operations select a visible row and use the
pane's visible **Actions** menu. Scenario setup and acceptance actions do not
call the Tauri bridge directly. File and protocol state may be read directly
only as a second assertion that the visible result corresponds to a real side
effect. AC-011 clicks the visible copy action and waits for its visible status
before reading the OS clipboard as that secondary assertion. The clipboard
reader has a 10-second subprocess ceiling and replaces command failures with a
generic error that cannot echo clipboard contents.

## Authoring and input rules

1. Register the stable ID, acceptance owner, platforms, input fidelity,
   fixture kind and any scenario-specific timeout in the typed catalog.
2. Add exactly one tagged entry spec in the matching directory.
3. Drive actions through visible user behavior. Prefer roles, accessible names
   and intentional state attributes over CSS classes or DOM position, except
   for third-party internals such as xterm output.
4. Assert both the visible result and the protocol side effect (for example,
   transferred file bytes or a preserved remote original).
5. Wait for observable state; do not add fixed sleeps.
6. Keep scenarios independent and safe to rerun. Do not depend on catalog order
   or a shared vault.
7. Keep `maxInstances: 1` until fixture and embedded-driver isolation have been
   proven under parallel application processes.

Controls are exercised through their real accessible input paths. File actions,
including the platform scenario's selected-file check, use the visible pane
**Actions** button. The renderer's `Shift+F10` handler has a focused component
test, while the native chord remains an explicit all-platform supplement.
Selects, forms and menu items are not changed by assigning DOM values,
dispatching synthetic events, calling `requestSubmit()` or invoking
`element.click()` in page JavaScript. The AST gate also rejects aliased/global
`execute` calls, Mocha retry overrides, right-button automation and raw
WebDriver keyboard-action construction. Automated primary-modifier chords go
through one runtime-guarded helper whose type and allowlist contain only A, T,
W, 1, 2 and comma; WebDriver special keys are rejected before reaching the
browser. Shortcuts use `Command` on macOS and `Control` on both Linux and
Windows. The automated AC-017 scenario presses T, W, 1, 2, comma and A through
the real host modifier. It deliberately does not claim pane transfer with
`Command`/`Control`+Left/Right: driver 1.2.0 loses the held modifier when it
maps an Arrow key. Those two native chords remain the typed manual supplement
above until the driver can deliver them faithfully. Windows never inherits a
WebKit-only compatibility shim.

The component check proves that a renderer event with `key="F10"` and
`shiftKey=true` opens the selected file's menu; it does not claim that a native
keyboard event reached the WebView. Driver 1.2.0 omits modifier flags when it
dispatches special keys, so `Shift+F10` stays manual-native. The same driver
sends secondary-button down/up without the `contextmenu` event that the app
receives from a native right-click. Separate all-platform supplements keep both
native paths explicit rather than replacing them with synthetic DOM events or
allowing false automated claims.

Native OS dialogs, biometric prompts, Finder drag-out and other
out-of-WebView surfaces remain manual-native. Automated scenarios do not
replace those surfaces with DOM or backend shortcuts.

## Result accounting and flake policy

The suite uses one purpose-built reporter and deliberately enables neither the
WDIO spec reporter nor JUnit. General reporters can persist raw WebDriver
command bodies, including `setValue` input, so their output is not an approved
scenario artifact. The custom JSONL reporter writes exactly three fields:

```json
{"scenarioId":"vault-lifecycle","status":"passed","durationMs":1234}
```

Errors, command bodies, skip reasons and fixture data are not serialized. Each
runner process gets its own `.artifacts/scenarios/results-<pid>.jsonl`, so
concurrent shards cannot overwrite or validate one another's accounting.

After WDIO exits, the runner requires exactly one result for every selected
scenario. A supported scenario must be `passed`; an unsupported scenario must
be exactly `skipped`. Missing, duplicate, unknown, failed or unexpectedly
skipped results fail the run. The green pipeline therefore cannot hide a new
skip or an empty tagged suite.

Mocha applies each scenario's own catalog timeout, with 120 seconds as the
default. A long selected flow therefore does not silently grant its larger
budget to every shorter spec. Long FTP, lifecycle and remote-edit flows declare
larger focused budgets instead of relying on one short global timeout.
WebDriver request retries, WDIO spec retries and Mocha retries are all zero: the
harness never replays a potentially non-idempotent click/key request or turns a
first red result green against the same mutated fixture. Product-level
retry/resume remains part of the transfer-resilience acceptance flow.

Each build subprocess has a 30-minute watchdog. The outer WDIO process gets the
sum of the selected scenario budgets plus startup/teardown overhead, and its
whole process tree is terminated if that deadline is exceeded. GitHub jobs also
have an aligned 180-minute ceiling, which exceeds the two 30-minute build
watchdogs plus the full-catalog runner budget and setup time. A driver or
compiler hang therefore cannot consume a runner indefinitely, while a slow run
that remains inside its declared component budgets is not killed early.
Fixture shutdown is bounded separately: after the graceful stdin-close window,
the runner uses `SIGKILL` or Windows `taskkill` and places a second deadline on
the final exit wait.

Failures attempt a timestamped screenshot under `.artifacts/scenarios/` for CI
diagnostics. Screenshot capture has a 15-second ceiling, so an unresponsive
WebView cannot hang teardown. Screenshots are diagnostic artifacts and must be
handled with the same access controls as any captured application screen.

## CI and releases

The same catalog and exact result accounting run on hosted macOS, Windows and
Linux GitHub runners; Linux uses Xvfb. Windows skips only the SSH-backed
scenarios because the fixture currently requires an OpenSSH daemon. Vault,
FTP, S3, remote-edit and platform-input scenarios still run through WebView2.
This is a repeatable CI regression gate, not a claim that Windows/Linux native
dialogs, biometrics, installed editors, display stacks or hardware diversity
have been validated on representative physical machines.

`scenarios:check` prints coverage per operating system instead of hiding those
declared gaps. macOS has 14/17 fully automated criteria, mixed AC-017, and two
manual-native owners. Linux has the same 14 fully automated plus mixed AC-017,
with AC-015 manual-native and AC-002 not applicable. Windows has 9/17 fully
automated plus mixed AC-017, reports AC-003/005/013/014/016 as five expected SSH
skips, and reports AC-002/015 as manual-native owners. An additional Windows
SSH fixture is still needed before those five criteria can become Windows
regression gates.

Normal CI runs for every pull request and every push to `main`; the `v*` tag
release workflow has its own equivalent `scenarios:check` and desktop-scenario
matrix. The release build jobs declare `needs: validate-scenarios`, so no draft
installer release is built until every supported scenario has passed and every
expected platform skip has been accounted for. In the normal CI Rust step,
Windows runs `cargo test --workspace --lib` because the top-level integration
fixtures are Unix-only; that does not disable its supported desktop scenarios.
Scenario diagnostics are uploaded per OS on failure or success whenever the
artifact directory is present; failures before artifact creation have nothing
to upload.

`@wdio/tauri-service` 1.2.0 still runs an external `tauri-driver` diagnostic
in embedded mode and may print a non-fatal `tauri-driver not found` line before
each passing spec. The embedded provider does not require that executable; do
not install or auto-install it merely to silence this diagnostic.
