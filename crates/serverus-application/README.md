# serverus-application

## Responsibility

`serverus-application` coordinates Serverus use cases through inward-facing
ports. It may be asynchronous, but it does not import Tauri, Tokio runtime APIs,
protocol SDKs, filesystems, or concrete adapters.

For the first vertical slice, `transfers`, `TransferCommandHandler` loads a
versioned domain aggregate, applies one pure domain event, commits the resulting
aggregate with optimistic concurrency, and sends the domain's declarative
effects to an outer runtime as one ordered batch.

The module owns coordination and application-level failure semantics. Transfer
state and transition policy remain in `serverus-domain`; storage and effect
execution remain behind ports.

## Public API

- `ApplyTransferEvent` is the application command.
- `TransferCommandHandler<R, D>` handles the command using two narrow ports.
- `TransferRepository` loads `VersionedTransfer` values and performs a
  compare-and-swap save against `TransferRevision`.
- `TransferEffectDispatcher` accepts an ordered `TransferEffectBatch` keyed by
  transfer ID and committed revision.
- `AppliedTransferEvent` returns the committed aggregate and revision.
- `TransferCommandError` distinguishes missing aggregates, invalid domain
  transitions, concurrent revisions, repository unavailability, and effect
  dispatch failure.

All transfer-specific ports stay in `serverus_application::transfers`; runtime
context and lifecycle ports belong in separate application modules.

## Commit and dispatch semantics

Commands for one transfer are sequenced through pending-batch recovery, load,
commit, and dispatch. Different transfer IDs remain independent. A failed
compare-and-swap emits no effects. A dispatcher failure occurs after commit, so
`EffectDispatchFailed` carries the complete committed batch; the handler keeps
that batch and retries it before a later command may commit another revision.

Dispatcher implementations must preserve vector order and use
`(transfer_id, revision)` as an idempotency key. Pending batches are scoped to
one handler and are intentionally ephemeral with the transfer runtime; a future
durable transfer repository must atomically enqueue them in an outbox.

## Dependencies

- `serverus-domain` for transfer aggregates, events, and effects;
- `async-trait` for object-safe asynchronous ports;
- `futures` for runtime-neutral per-transfer async sequencing;
- `thiserror` for typed, safe application errors.

No Transfers application error contains adapter diagnostics, paths,
credentials, or other secret material.

## Invariants

- Domain transitions happen only after a successful versioned load.
- Invalid or missing transfers are never saved and emit no effects.
- Saves use the revision returned by the same load as their CAS precondition.
- Effects are dispatched only after a successful save and in domain order.
- Revisions for one transfer cannot overtake each other during dispatch.
- A domain no-op is returned at the loaded revision without another save or
  effect dispatch, so idempotent commands stay side-effect free.
- Concurrent-save rejection emits no effects.
- Post-commit dispatch failure retains the exact batch required for recovery.

## Tests

Run the slice in the root workspace:

```bash
cargo test -p serverus-application
cargo clippy -p serverus-application --all-targets -- -D warnings
```

`tests/transfers.rs` uses local in-memory fakes to prove persistence ordering,
multi-effect order, invalid and missing aggregate handling, concurrent revision
rejection, typed repository/dispatcher failures, concurrent pause/resume
sequencing, and recovery of a committed batch before the next revision.
