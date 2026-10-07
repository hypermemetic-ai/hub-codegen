# hub-codegen tests

```bash
cargo test                              # everything: unit tests in src/ and every file here
cargo test --test typescript_codegen_test
cargo test --test cache_invalidation_test -- --nocapture
scripts/test-cache.sh [--verbose] [--scenario a|b|c|all]   # the two cache suites below
```

No test needs a running backend. A few compile what they generate:
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
