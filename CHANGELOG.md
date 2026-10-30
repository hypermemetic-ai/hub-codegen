# Changelog

Releases before 0.5.2 are the repository's version tags (0.1.0 to 0.5.0) and
NAMING-10's merge (0.5.1, the rename).

## 0.5.2 — 2026-10-30

- TICKETS-37: a type the protocol root declares and a generated namespace
  uses — an IR 2.0's namespace `""`, where synapse ≤ 3.16 keeps a type the
  root's schema shares with an activation's (since SYNAPSE-CC-9, menger's
  `Scope` and `MethodPath`) — is generated in the namespace that refers to it
  most, a tie to the shortest name and then the first (IR 2.1's rule), and
  every other namespace imports it from there; the types it names follow it.
  0.5.1 wrote the reference, defined the type nowhere and imported it nowhere
  (TS2304). TypeScript; an IR 2.1 is unchanged by it
  (`src/service_types.rs`, `tests/tickets37_service_types_test.rs`).
