# Changelog

Releases before 0.5.2 are the repository's version tags (0.1.0 to 0.5.0) and
NAMING-10's merge (0.5.1, the rename).

## Unreleased

- RETURNS-46: every namespace gets its aggregate, `{Ns}Event` (`{Ns}Aggregate`
  when the namespace has a type of that name), in `<ns>/events.ts`, which
  `<ns>/index.ts` re-exports: one member per method, `{ method; event: I }`
  (`Result<T, E>` for a method with an error type), one per static child
  carrying the child's aggregate, one per dynamic child carrying
  `{ name: string; event }` — the Rust aggregate's JSON. A dynamic child's
  event is `unknown`: IR 3.0 names a dynamic child but describes none of its
  methods.
- RETURNS-46, at run time: every generated call's `.events()` yields its
  client's aggregate, as Rust's `{Ns}Event` serialises it — a root call
  `{ method: 'count', event }`, a call through a static child's accessor
  nested, `{ method: 'branch', event: { method: 'grow', event } }` — and so
  does `watch(...)` over generated streams: one shape whatever the level.
  `MethodStream` gains a third type parameter `A` (default
  `MethodEvent<P, E>`, what a stream made by hand with no lift still yields)
  and an optional `lift`; rpc.ts exports `Up`, watch.ts `LiftableSource`.

### Changed

- RETURNS-46, **what moved**: a generated stream's `.events()` was
  `{ method: '<path>', event }` (`MethodEvent<'tickets.watch', E>`); it is the
  client's aggregate, `{ method: 'watch', event }` (`TicketsEvent`), or the
  parent's through an accessor. A `watch` over generated streams (not
  `[path, stream]` pairs, which are unchanged) yields
  `{ source: '<path>', event: <aggregate> }` where it yielded the item: the
  path is the item's `source`, and the stream's own `.method`. menger and
  plexus-ui have no TypeScript reading `.events()`.
- RETURNS-46: a namespace client that streams, has static children or is
  one is `XClient<A = undefined>`, with `createXClient<A>(rpc, lift?)`; its
  streaming methods return `MethodStream<'p', T, Up<A, XEvent>>`, and a
  static child's property is `YClient<Up<A, XEvent>>` (was `YClient`). A
  client that does none of these is unchanged.

## 0.6.0 — 2026-11-02

RETURNS-36 brings the returns/0.7 line over main, and RETURNS-15 with it:

- RETURNS-15: IR 3.0 is read — `mdReturns` the item, `mdError` and
  `mdProgress` (new `MethodDef.md_error` / `md_progress`), `mdCardinality`
  (`"one"` / `"many"`, the new `MethodDef.md_cardinality`, which replaces
  `md_streaming`; `MethodDef::is_streaming()`). IR 2.0 and 2.1 are read as
  before: `mdStreaming` becomes the cardinality. Any other version is refused,
  naming the three (`ir::check_ir_version`). The generators branch on the
  cardinality, so a method generates as it did from either version; the
  error and progress types are generated as types and typed on the method by
  RETURNS-17 / RETURNS-18.
- IR 2.1's forms, which a 3.0 from synapse's main carries too, are read:
  `RefMap` (TS `Record<string, V>`, Rust `BTreeMap<String, V>`), `RefTuple`
  (`[A, B]`, `(A, B)`), `RefJson` (the 2.0 `RefAny`), `RefRefined` (its
  base), `RefChild` (never typed: the role generates the child), `keRepr`
  external and untagged enums with `vdRef` (TS unions of `{ name: … }` or of
  the variants' types; Rust serde's default or `#[serde(untagged)]`), and
  `mdInjected` (a param the server supplies: documented, never sent). The
  rest of SYNAPSE-CC-10 (the coverage report and its refusals, integer
  widths, `irRefusals`) is not in this release.
- RETURNS-16: a tagged Rust enum renames each variant to its exact wire tag
  (`status_changed` decodes; `rename_all = "lowercase"` read it as
  `statuschanged`).
- RETURNS-22: the repository holds service-codegen and nothing stale; a
  generated smoke test has no `ws://localhost:4444` default.
- RETURNS-23: the output is deterministic — namespace members, warnings and
  the JSON envelope's maps in a fixed order.

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
