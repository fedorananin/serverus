# serverus-testkit

## Responsibility

Reusable deterministic test doubles and contract-test support shared across
the workspace. Fakes live here when more than one crate benefits from the same
behavioral implementation; one-off test helpers stay next to their test.

## Public API

- `DeterministicContextIds` supplies an explicit finite generation sequence.
- `RecordedContextEvents` records typed events in publication order.
- `RecordedContextCleanup` records retired generations and supports one-shot
  deterministic failure injection.

The public surface grows with extracted ports and reusable adapter contracts,
not with implementation-specific mocks.

## Dependencies

Production crates must not depend on this crate outside `dev-dependencies`.
The testkit may depend inward on application and domain contracts, but never on
Tauri or production protocol adapters.

## Invariants

- IDs, failures, time, and recorded effects are deterministic.
- Fakes preserve the same semantic contract as the corresponding real port.
- Test helpers never require a WebView, network, real filesystem, or wall-clock
  delay unless they are explicitly infrastructure fixtures.

## Tests

```bash
cargo test -p serverus-testkit
cargo clippy -p serverus-testkit --all-targets -- -D warnings
```
