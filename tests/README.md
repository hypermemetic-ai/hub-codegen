# service-codegen tests

```bash
cargo test                              # everything: unit tests in src/ and every file here
cargo test --test typescript_codegen_test
cargo test --test cache_invalidation_test -- --nocapture
scripts/test-cache.sh [--verbose] [--scenario a|b|c|all]   # the two cache suites below
```

No test needs a running service, but for `bidir19_input_test`'s live run, which starts one when it is there. A few compile what they generate:
`rust_codegen_smoke_test` and `ir9_dynamic_child_test` run `cargo check` on a
generated Rust client, and `ir9_dynamic_child_test` runs `tsc` when it is on
`PATH`.

## Suites

| file | covers |
| -- | -- |
| `typescript_codegen_test.rs` | TS invariants: transport dispatch (`ws` / `browser` / `none`), each `--generate` selector's file set, the plugin filter, the smoke tests' backend URL (`PLEXUS_URL`, `--backend-url`), three-way merge (skip preserves a user edit, force overwrites it) |
| `rust_codegen_smoke_test.rs` | the generated Rust client compiles, has the expected structure and no warnings |
| `cache_invalidation_test.rs` | the code-cache manifest (`src/cache.rs`) and hashing (`src/hash.rs`): method-only, children-only and combined IR changes (scenarios A–C), manifest read/write, hit/miss, an end-to-end cache cycle, hash speed |
| `configurable_backend_test.rs` | a mock backend that builds IR from the JSON configs in `test_scenarios/`, driven through scenarios A–C |
| `ir7_deprecation_test.rs` | deprecation annotations (TS `@deprecated`, Rust `#[deprecated]`), stderr warnings, `--fail-on-deprecated`, `--no-deprecation-annotations` |
| `ir9_dynamic_child_test.rs` | typed handles for dynamic children (`DynamicChild`, `Listable`, `Searchable`) in TS and Rust |
| `r4_credential_surface_test.rs` | typed credential requirements on generated clients; golden snapshots in `golden/r4/` |
| `ca2_ts_auth_runtime_test.rs` | the generated TS transport's credential provider and schema-directed attachment; golden snapshot in `golden/ca2/` |
| `req7_typed_errors_test.rs` | typed error classes for the four semantic JSON-RPC error codes in the generated transport |
| `req9_jsdoc_test.rs` | per-method JSDoc from each parameter's `pd_source` |
| `auth9_grant_surface_test.rs` | each method's grant, target and long description in the generated docs (AUTH-9) |
| `synapse_cc_2_injected_params_test.rs` | a parameter the server injects is never sent, only documented (SYNAPSE-CC-2) |
| `polyglot12_wire_names_test.rs` | the TS transport's types name the wire's fields as the wire does (POLYGLOT-12) |
| `returns16_variant_tags_test.rs` | the Rust generator renames each variant to its exact wire tag (RETURNS-16) |
| `returns17_method_stream_test.rs` | a streaming method returns `MethodStream<'ns.m', E>`; a failed call throws a typed `RemoteCallError` (RETURNS-17) |
| `returns23_deterministic_output_test.rs` | the output is deterministic: namespaces, warnings and file lists in a fixed order (RETURNS-23) |
| `returns38_watch_test.rs` | `watch(a, b, c)` in the generated TS client (RETURNS-38) |
| `bidir19_input_test.rs` | a method with an input half (`mdInput`): TS and Rust goldens in `golden/bidir19/`, every method without input byte-identical, `tsc --strict` with `ts_input/types.test.ts`, and `ts_input/live.test.ts` against a real service with `Input<Item>` / `Input<Bytes>` methods (`$BIDIR_FIXTURE_SERVER`; skipped without it unless `SERVICE_CODEGEN_REQUIRE_BIDIR=1`) (BIDIR-19) |
| `tickets37_service_types_test.rs` | a type the protocol root declares is generated where it is used most and imported elsewhere (TICKETS-37) |
| `json_keys.rs` | the JSON shared with synapse-cc reads the old keys and writes the new (NAMING-10) |
| `naming_gate.rs` | `scripts/naming-gate.sh`: every name says service (NAMING-10) |

## Fixtures

- `golden/` — byte-for-byte expected output; a generator change that alters
  one of these files is a visible diff in review (`UPDATE_GOLDEN=1 cargo test`
  rewrites them).
- `test_scenarios/` — backend configs for scenarios A (methods change),
  B (children change) and C (both); [its README](test_scenarios/README.md)
  has the format. The examples read them too:

  ```bash
  cargo run --example generate_from_config tests/test_scenarios/scenario_a_initial.json
  cargo run --example compare_configs tests/test_scenarios/scenario_a_initial.json tests/test_scenarios/scenario_a_modified.json
  ```

The separate self / children hashes in the cache suites are computed by the
test helpers; the code cache itself keys each plugin on one `irHash`
([caching and merge](../docs/architecture/caching-and-merge.md),
[cache contracts](../docs/architecture/CACHE_CONTRACTS.md)).
