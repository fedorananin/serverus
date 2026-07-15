---
status: accepted
---

# Use layered tests with reusable fakes and adapter contracts

Serverus will place most behavioral coverage below Tauri, the WebView, real
time, and real networks. Pure domain tests cover state machines and
invariants; application tests run use cases against reusable fake ports;
runtime tests exercise ownership, races, cancellation, reconnect, and timeouts
with deterministic time; capability contract suites are shared by the
in-memory endpoint and applicable protocol adapters. Existing real SSH/SFTP,
FTP, S3, vault, filesystem, tunnel, tar, and remote-edit integration tests
remain as infrastructure proof. Frontend feature models use an injectable API
and event source, while only a small set of critical desktop flows belongs in
end-to-end tests.

## Considered options

- Rely mainly on infrastructure integration and desktop end-to-end tests.
  This validates wiring but makes failures slow, broad, and difficult to
  reproduce deterministically.
- Mock implementation modules separately in each test. This creates brittle
  tests and fakes whose behavior drifts from the real adapters.
- Use the layered strategy above, with `serverus-testkit` providing builders,
  deterministic clocks/IDs, recorded events, in-memory ports, fixtures, and
  reusable contract suites.

## Consequences

- A bug fix receives a regression test at the lowest layer that can reproduce
  the defect; an end-to-end discovery does not require an end-to-end regression.
- Fakes must pass the same applicable contracts as real adapters.
- Retry, timeout, and race tests never wait on wall-clock sleeps.
- CI does not hide flaky tests with retries or ignored failures.
- Binding generation and architecture checks are deterministic verification
  gates, and `npm run verify` runs the full local safety net.
- Property, mutation, and fuzz testing are introduced selectively for parsers,
  paths, migrations, state transitions, and other high-value pure surfaces.

The exact test layers and migration order are defined in
[ARCHITECTURE.md](../../ARCHITECTURE.md#testing-strategy).
