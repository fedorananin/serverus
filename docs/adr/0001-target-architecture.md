---
status: accepted
---

# Adopt a modular desktop monolith with explicit effect boundaries

Serverus is a stateful desktop runtime, not a CRUD application: it owns an
encrypted vault, live SSH/FTP/S3 sessions, terminals, tunnels, transfers,
watchers, and background work. We will keep one Tauri application, one native
process, one Tokio runtime, and one Svelte WebView, while incrementally moving
the Rust code into a Cargo workspace with `serverus-domain`,
`serverus-application`, `serverus-runtime`, `serverus-adapters`,
`serverus-testkit`, and the existing `src-tauri` desktop composition root.

The domain is a synchronous functional core. Application use cases coordinate
domain decisions through effect ports. The Tokio runtime owns long-lived
mutable state through actor-like supervisors and generation-scoped runtime
contexts. Infrastructure implements inward-facing ports, and Tauri only maps
IPC DTOs, invokes an `ApplicationHandle`, and emits mapped application events.
Remote protocols expose composable capabilities rather than leaking protocol
switches into use cases. The Svelte frontend is organized by feature and uses
one injectable `AppApi` and `AppEventSource` instead of importing generated
commands from feature code.

## Considered options

- Keep the current manager-oriented single crate. This has low short-term
  cost, but it cannot compile-time protect the boundaries that most need
  protection and keeps lifecycle orchestration difficult to test.
- Rewrite the application around a framework or split it into services. This
  would introduce migration and operational risk without a product need for
  multiple processes or distributed consistency.
- Adopt the modular monolith described above. This preserves the deployment
  model and permits one behavior-preserving vertical migration at a time.

## Consequences

- Migration is incremental; old managers may sit behind adapters until their
  bounded context is extracted. A big-bang rewrite is prohibited.
- Dependency direction is enforced by Cargo and CI, not only by documentation.
- Every long-lived task has one runtime owner and structured cancellation.
- Vault lock and vault switch have intentionally different lifecycle semantics.
- Tauri, Specta, protocol SDKs, persistence records, and IPC DTOs remain outside
  the domain and application core.
- Microservices, a mandatory daemon, event sourcing, and a trait per type are
  explicitly out of scope.

The complete boundaries, invariants, and migration gates are defined in
[ARCHITECTURE.md](../../ARCHITECTURE.md).
