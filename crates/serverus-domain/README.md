# serverus-domain

`serverus-domain` is Serverus's synchronous, effect-free domain core. The first
vertical slices model the Transfers lifecycle as a pure state machine and the
value semantics of a Vault-bound runtime-context generation. Callers submit
semantic events and receive state/effect decisions for outer layers to execute.

## Responsibility

This crate owns transfer state, transition rules, retry-budget accounting,
conflict decisions, cancellation semantics, and terminal outcomes. It does not
perform I/O, spawn tasks, sleep, emit Tauri events, choose a remote protocol, or
define persistence and IPC schemas.

## Public API

The `transfers` module exposes:

- `TransferId`, `AttemptNumber`, and `RetryBudget` value objects;
- `Transfer`, created in `Queued` state with `Transfer::queued`;
- `TransferEvent` for semantic input and `TransferState` for current state;
- `Transfer::transition`, a pure operation returning `Transition`;
- `TransferEffect` values that an application/runtime layer interprets;
- typed conflict, failure, completion, terminal, and invalid-transition values.

The `runtime_context` module exposes non-zero `RuntimeContextId`, validated
`VaultKey`, lock/unlock state, and same-generation Vault reidentification.
Cancellation tokens and cleanup ownership stay in `serverus-runtime`.

`Transition::next` is a new aggregate. The original `Transfer` remains
unchanged, which lets application tests inspect decisions without a runtime or
mocks.

## Dependencies

The crate currently has no third-party dependencies and uses only `std`. Keep
Tauri, Tokio, Specta, protocol SDKs, filesystems, clocks, channels, mutexes,
serialization records, and generated DTOs outside this crate.

## Invariants

- Transfer IDs and attempt numbers are non-zero and strongly typed.
- A transfer starts queued; the first running attempt is number one.
- A retry budget counts retries after the initial attempt and is never exceeded.
- Pause/resume and conflict overwrite preserve the active attempt number.
- Overwrite, rename, and skip remain distinct conflict decisions.
- Repeated cancellation is an effect-free no-op once cancellation is already
  in progress or the transfer is terminal.
- Active cancellation is two-phase: request worker cancellation, then enter the
  terminal `Cancelled` state only after acknowledgement.
- Cancelling a scheduled retry explicitly cancels its timer.
- `Completed`, `Cancelled`, and `Failed` states reject further events.
- Effects describe required work; domain transitions never execute that work.

## Tests

Run the crate in isolation:

```bash
cargo test --manifest-path crates/serverus-domain/Cargo.toml
cargo clippy --manifest-path crates/serverus-domain/Cargo.toml --all-targets -- -D warnings
```

The integration-style domain tests in `tests/transfers.rs` cover value-object
validation and every supported lifecycle branch without mocks or external
resources.
