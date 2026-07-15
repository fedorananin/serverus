# serverus-runtime

## Responsibility

`serverus-runtime` owns process-lifetime coordination and generation-scoped
resources. It is the outer lifecycle layer around application use cases; domain
rules and protocol implementations do not live here.

## Public API

`ApplicationHandle` exposes a small, cloneable facade over one active
`RuntimeContext`:

- activating the same Vault unlocks and reuses its generation;
- a different Vault cannot become active until a switch permit is committed;
- locking revokes secret-requiring access but preserves the generation;
- moving the same Vault updates its key without invalidating owned work;
- beginning a switch blocks new leases; dropping the permit restores the old
  context, while committing it cancels generation leases, runs cleanup, and
  retires the generation;
- leases from a retired generation fail validation, preventing late async work
  from mutating a newly selected Vault.

Cleanup and event delivery are application ports, so lifecycle tests use no
Tauri window, filesystem, network, or protocol SDK.

## Dependencies

The runtime depends inward on `serverus-application` and `serverus-domain` and
uses Tokio only for lifecycle ownership and cancellation. It never depends on
Tauri, a protocol SDK, or `serverus-adapters`.

## Invariants

- One coordinator owns the current runtime-context generation.
- Vault lock preserves the generation; Vault switch retires it.
- Beginning a switch blocks new leases and dropping an uncommitted permit
  restores the exact previous context.
- A committed switch cancels old leases before cleanup and never restores a
  retired generation, including when cleanup reports failure.
- A stale lease cannot validate against a later context.

## Tests

```bash
cargo test -p serverus-runtime
cargo clippy -p serverus-runtime --all-targets -- -D warnings
```
