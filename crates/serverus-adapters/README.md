# serverus-adapters

## Responsibility

Concrete infrastructure implementations of inward-facing application ports.
The initial adapter generates opaque non-zero `RuntimeContextId` values from
random UUID v4 values. Protocol clients, filesystem persistence, and Tauri
delivery belong here as their application ports are extracted. Tauri-specific
mapping remains in `src-tauri`; this crate contains reusable infrastructure.

## Public API

- `UuidRuntimeContextIdGenerator` implements the application-owned
  `RuntimeContextIdGenerator` port.

New adapters expose only the inward-facing port contract, not their concrete
SDK handle.

## Dependencies

This crate may depend on `serverus-application` and `serverus-domain`; neither
of those inner crates may depend back on adapters.

## Invariants

- Generated runtime-context identifiers are opaque and non-zero.
- Protocol, filesystem, crypto, and OS diagnostics do not become domain or IPC
  errors by accident.
- Inner crates never import this crate.

## Tests

```bash
cargo test -p serverus-adapters
cargo clippy -p serverus-adapters --all-targets -- -D warnings
```

Adapter extraction adds contract tests alongside each real implementation.
