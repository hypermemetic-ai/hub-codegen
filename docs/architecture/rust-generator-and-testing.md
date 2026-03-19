# Rust Generator and Testing Architecture

## Overview

Hub-codegen includes a Rust client generator alongside the TypeScript generator. Both generators share the same input interface (`GenerationOptions`, `IR`) and output interface (`GenerationResult`). The Rust generator is conditionally compiled via the `rust` Cargo feature.

---

## Generator Interface

**File**: `src/generator/mod.rs`

There is no explicit Rust trait — generators follow the convention:

```rust
pub fn generate(ir: &IR) -> Result<GenerationResult>
// or
pub fn generate(ir: &IR, options: &GenerationOptions) -> Result<GenerationResult>
```

Both generators are dispatched from `main.rs` based on `--target`:

```rust
match target {
    CodegenTarget::Typescript => hub_codegen::generate_typescript(&ir, &options)?,
    CodegenTarget::Rust       => hub_codegen::generate_rust(&ir)?,
}
```

### Shared Types

```rust
pub struct GenerationOptions {
    pub transport: TransportEnv,         // Ws | Browser | None
    pub generate: GenerateSelector,      // All | Transport | Rpc | Plugins | Smoke | Package
    pub plugins_filter: Option<Vec<String>>,
    pub smoke_transport_path: String,
    pub backend_url: String,
}

pub struct GenerationResult {
    pub files: HashMap<String, String>,            // rel_path → content
    pub warnings: Vec<Warning>,
    pub file_hashes: HashMap<String, String>,      // rel_path → SHA-256[..16]
    pub dependencies: HashMap<String, String>,     // runtime npm/crate deps
    pub dev_dependencies: HashMap<String, String>,
}
```

The Rust generator ignores `GenerationOptions` and uses only `&IR` — it has no transport variants or artifact selectors.

---

## What the Rust Generator Produces

**Files**: `src/generator/rust/`

### Output File Set

| File | Contents |
|------|----------|
| `src/lib.rs` | Module declarations + re-exports |
| `src/types.rs` | Core transport types (`PlexusStreamItem` only) |
| `src/client.rs` | Base `PlexusClient` with WebSocket transport |
| `src/{namespace}/mod.rs` | All types and method functions for the namespace |
| `Cargo.toml` | Static manifest with fixed dependencies |

### `src/types.rs`

Contains only the `PlexusStreamItem` enum — the universal wire-format envelope. This is the Rust equivalent of the TypeScript `types.ts` but much smaller (no `StandardRequest`/`StandardResponse` bidirectional types yet).

### `src/client.rs`

The base `PlexusClient` struct with WebSocket transport using `tokio-tungstenite`. Provides:
- `call_stream(method, params)` → `Pin<Box<dyn Stream<Item = Result<T>> + Send>>`
- `call_single(method, params)` → `Result<T>`

These unwrap `PlexusStreamItem` internally — callers receive domain types directly.

### `src/{namespace}/mod.rs`

Each namespace becomes a module with:
1. Cross-namespace `use crate::...` imports (absolute crate paths)
2. Domain type definitions:
   - Structs → `#[derive(Debug, Clone, Serialize, Deserialize)] pub struct ...`
   - Enums → `#[derive(Debug, Clone, Serialize, Deserialize)] #[serde(tag = "type", rename_all = "snake_case")] pub enum ...`
   - Variant fields have **no `pub`** (accessed via pattern match)
3. Standalone async functions per method:
   ```rust
   pub async fn echo(client: &PlexusClient, count: i64, message: String)
     -> Pin<Box<dyn Stream<Item = Result<EchoEvent>> + Send>>
   ```
   - Streaming: returns `Pin<Box<dyn Stream<...> + Send>>`
   - Non-streaming: returns `Result<T>`

**Design choice — standalone functions, not methods**: Avoids name conflicts across 50+ namespaces where many namespaces define a method like `list` or `get`.

### `Cargo.toml`

Static fixed manifest (not dynamic like `package.json`):
```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = "0.20"
futures = "0.3"
async-stream = "0.3"
anyhow = "1"
```

No equivalent to npm transport variants — Rust always uses `tokio-tungstenite`.

---

## Differences from TypeScript Generator

| Aspect | TypeScript | Rust |
|--------|-----------|------|
| Streaming return type | `AsyncGenerator<T>` | `Pin<Box<dyn Stream<Item = Result<T>> + Send>>` |
| Method organization | Class (`{Ns}ClientImpl`) | Standalone functions in namespace module |
| Type definitions | Interface / discriminated union | `struct` / `enum` with `#[serde(tag = "type")]` |
| Cross-namespace imports | Relative (`../../other/types`) | Absolute (`use crate::other::OtherType`) |
| Rust keyword escaping | N/A | `r#` prefix (e.g., `r#type`) |
| Dependencies | Dynamic npm (transport-dependent) | Static `Cargo.toml` |
| Transport variants | 3 modes (ws/browser/none) | Single (tokio-tungstenite) |
| Generation options | Full `GenerationOptions` used | Only `&IR` used |

---

## Integration Tests

### `tests/rust_codegen_smoke_test.rs`

Three integration tests:

1. **`test_generated_rust_compiles()`** — Generates code from a test IR fixture, writes to a temp directory, runs `cargo check`. Passes only if the generated Rust is syntactically and semantically valid.

2. **`test_generated_code_structure()`** — Validates the file set and checks key signatures (function names, return types, struct definitions) without actually compiling.

3. **`test_no_warnings()`** — Ensures clean generation: `GenerationResult.warnings` is empty for the test IR.

### `tests/configurable_backend_test.rs`

Tests the hash stability and cache invalidation system with a "mock backend" that generates IR dynamically:

- Tests that `ir_hash` (the IR's own stable content hash) changes when methods/children change
- Validates Scenario A (method-only change), Scenario B (children-only), Scenario C (both)
- Uses `compute_ir_hash()` directly to verify hash algebra

### `tests/typescript_codegen_test.rs`

TypeScript-specific integration tests covering:
- Transport variant dispatch (ws / browser / none)
- Artifact consistency (test scripts use `bun test`, smoke tests import from `bun:test`)
- Generate selector correctness (exact file counts per selector)
- Plugin filtering (exact match, prefix match, dot-segment awareness)
- Three-way merge (skip vs. force strategy)

### Test Scenarios (`tests/test_scenarios/`)

JSON IR fixtures for cache invalidation testing:

| File | Description |
|------|-------------|
| `scenario_a_initial.json` | Plugin with `[method1, method2, Type1]` |
| `scenario_a_modified.json` | Same plugin with `method2` removed |
| `scenario_b_initial.json` | Plugin with methods + `[child1, child2]` |
| `scenario_b_modified.json` | Same with `child2` removed |
| `scenario_c_initial.json` | Both methods and children |
| `scenario_c_modified.json` | Both modified |

---

## Unit Tests (`src/generator/rust/tests.rs`)

Four focused unit tests using a simple test IR (health namespace, `Status` struct, `Event` enum, `check`/`watch` methods):

1. Type generation correctness
2. Client generation structure
3. Full generation (all files present, correct content)
4. Case conversion helpers (`to_snake_case`, `to_pascal_case`)

---

## Key Design Decisions

### Namespace-Scoped Types

Each namespace is a separate module to avoid duplicate type names — `Status` in the `health` namespace and `Status` in the `arbor` namespace become `health::Status` and `arbor::Status`.

### Fallback for Missing Types

When a type is referenced in IR but not defined (schema gap), the generator emits `serde_json::Value` as the field type. This allows compilation to succeed while flagging the gap as a warning in `GenerationResult.warnings`.

### Hierarchical Files via Namespace Paths

Functions `parse_namespace_path()` and `namespace_to_file_path()` convert dotted namespace names to `src/solar/earth/luna/mod.rs` paths.

### Automatic Cross-Namespace Imports

`collect_cross_namespace_imports_hierarchical()` scans all type references in a namespace and emits the appropriate `use crate::...` imports, preventing manual maintenance.

---

## Cargo Feature Gating

```toml
[features]
default = ["typescript"]
typescript = []
rust = []
all = ["typescript", "rust"]
```

If `--target rust` is used but the `rust` feature is not compiled in, `main.rs` bails with a descriptive error rather than panicking.
