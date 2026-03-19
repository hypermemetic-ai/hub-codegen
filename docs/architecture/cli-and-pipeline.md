# hub-codegen CLI and Pipeline Architecture

## Overview

**hub-codegen** is a pure, stateless Rust CLI that transforms Synapse IR (Intermediate Representation) JSON into language-specific client code. It reads IR from stdin or a file, applies generation options (transport type, target language, artifact selector), generates code files and metadata, and outputs them either directly to disk (files mode) or as structured JSON (json mode). The CLI supports multiple target languages via Cargo feature flags, with TypeScript as the default. It is designed to be embedded in orchestration tools (like synapse-cc) that handle merge decisions, caching, and dependency management.

---

## CLI Entry Point

The main CLI is defined in `src/main.rs` using clap derive macros for parsing command-line arguments.

### Positional Arguments

- **`input`** (default: `"-"`) — Path to IR JSON file. Use `-` to read from stdin.

### Option Flags

#### Output & Path Control

- **`-o, --output <PATH>`** (default: `"./generated"`) — Output directory for `--output-format files` mode. Ignored in json mode.

#### Language & Target

- **`-t, --target <TARGET>`** (default: `"typescript"`) — Target language: `typescript` or `rust`. Language-specific generator modules are conditionally compiled via Cargo features.

#### Transport Configuration

- **`--transport <TYPE>`** (default: `"ws"`) — Transport environment for generated code:
  - `ws` — Node.js/test environment: imports WebSocket from `'ws'` npm package, includes `@types/ws` dev dependency, tsconfig includes `"types": ["node"]`
  - `browser` — Native browser environment: uses native `window.WebSocket`, no `'ws'` import, no `ws` dependency, tsconfig uses `"lib": ["ES2022", "DOM"]`
  - `none` — External RPC client: no transport.ts generated, code assumes `@plexus/rpc-client` is provided externally as workspace dependency

#### Code Generation Selector

- **`--generate <SELECTOR>`** (default: `"all"`) — Which artifact subset to produce (TypeScript only):
  - `all` — All artifacts: types.ts, rpc.ts, transport.ts, index.ts, namespace plugins, package.json, tsconfig.json, smoke test, .codegen-metadata.json
  - `transport` — Protocol types + transport only: types.ts, rpc.ts, transport.ts (IR not required; transport template is static)
  - `rpc` — Core RPC layer: types.ts, rpc.ts, index.ts (no transport, no plugins, no package.json)
  - `plugins` — Plugin namespace files only: each namespace gets types.ts, client.ts, index.ts
  - `smoke` — Schema walk smoke test: smoke.ts script (no test framework)
  - `package` — package.json only (computed from code hash)

#### Plugin Filtering

- **`--plugins <NAMES>`** (comma-separated, e.g., `"echo,health"`) — Optional filter for `--generate plugins`. When used with `--generate all`, automatically resolves type dependencies: requested plugins get full generation (client.ts + types.ts), type-dependency namespaces get types.ts only.

#### Merge Strategy (Files Mode Only)

- **`--merge-strategy <STRATEGY>`** (default: `"skip"`) — How to handle user-modified files during three-way merge:
  - `skip` — Safe default. Skip writing files where `cached_hash != current_hash` (user modification detected).
  - `force` — Overwrite all files regardless of user modifications.
  - `interactive` — Interactive prompts (not yet implemented).

#### Output Format

- **`--output-format <FORMAT>`** (default: `"files"`) — Selects the entire output pipeline:
  - `files` — Write generated files to disk at `--output`, perform three-way merge, maintain cache manifest at `~/.cache/plexus-codegen/hub-codegen/<target>/<backend>/manifest.json`, write starter package.json if not present, print merge summary to stderr
  - `json` — Emit structured JSON to stdout (no file writes). Content includes generated files, file hashes, warnings, versions, and dependencies. Used by synapse-cc.

#### Smoke Test Configuration

- **`--smoke-transport-path <PATH>`** (default: `"../transport"`) — Import path for PlexusRpcClient in generated smoke tests.
- **`--backend-url <URL>`** (default: `"ws://localhost:4444"`) — WebSocket URL embedded as fallback in generated smoke tests.

#### Runtime Flags

- **`--dry-run`** — Files mode only. Print generated files to stdout without writing to disk.

---

## High-Level Code Flow

### Phase 1 — Parse IR

Read IR JSON from file or stdin (`main.rs:120–131`). For transport-only generation (`--generate transport`), use a minimal dummy IR since transport.ts is a static template.

### Phase 2 — Parse CLI Options → GenerationOptions

Convert clap-parsed arguments into the internal `GenerationOptions` struct (`main.rs:134–153`).

### Phase 3 — Generate Code

Dispatch to target language generator (`main.rs:156–170`):

```
TypeScript: hub_codegen::generate_typescript(&ir, &options)?
Rust:       hub_codegen::generate_rust(&ir)?
```

Returns a `GenerationResult` containing:
- `files: HashMap<String, String>` — Relative path → file content
- `warnings: Vec<Warning>` — Location + message for schema gaps
- `file_hashes: HashMap<String, String>` — Relative path → SHA-256[..16] hash
- `dependencies: HashMap<String, String>` — Runtime npm packages
- `dev_dependencies: HashMap<String, String>` — Dev npm packages

### Phase 4 — Output

Branch on `--output-format` (`main.rs:181–293`).

#### JSON Mode (`main.rs:182–195`)

Emit to stdout as a single JSON object — no file I/O, no merge, no cache writes:

```json
{
  "files": { "types.ts": "...", "rpc.ts": "...", ... },
  "fileHashes": { "types.ts": "abc1..." },
  "warnings": [ { "location": "cone.chat", "message": "..." } ],
  "hubCodegenVersion": "0.2.0",
  "dependencies": { "ws": "^8.18.0" },
  "devDependencies": { "typescript": "^5.0.0" }
}
```

#### Files Mode (`main.rs:196–292`)

1. Create output directory
2. Write starter package.json if not present (user owns it after first run)
3. Load cache manifest from `~/.cache/plexus-codegen/<target>/<backend>/manifest.json`
4. Three-way merge: new generated files vs. disk vs. cache baseline
5. Update and write cache manifest

---

## Key Types and Enums

### CLI Types (`src/main.rs`)

```rust
enum CodegenTarget   { Typescript, Rust }
enum OutputFormat    { Files, Json }         // default: Files
enum CliTransport    { Ws, Browser, None }   // default: Ws
enum CliGenerate     { All, Transport, Rpc, Plugins, Smoke, Package }  // default: All

struct CodegenOutput<'a> {
    files: &'a HashMap<String, String>,
    file_hashes: &'a HashMap<String, String>,
    warnings: Vec<WarningOutput<'a>>,
    hub_codegen_version: &'static str,
    dependencies: &'a HashMap<String, String>,
    dev_dependencies: &'a HashMap<String, String>,
}
```

### Library Public Types (`src/lib.rs`, `src/generator/mod.rs`)

```rust
pub enum TransportEnv     { Ws, Browser, None }
pub enum GenerateSelector { All, Transport, Rpc, Plugins, Smoke, Package }

pub struct GenerationOptions {
    pub transport: TransportEnv,
    pub generate: GenerateSelector,
    pub plugins_filter: Option<Vec<String>>,
    pub smoke_transport_path: String,
    pub backend_url: String,
}

pub struct GenerationResult {
    pub files: HashMap<String, String>,
    pub warnings: Vec<Warning>,
    pub file_hashes: HashMap<String, String>,
    pub dependencies: HashMap<String, String>,
    pub dev_dependencies: HashMap<String, String>,
}

pub struct Warning { pub location: String, pub message: String }
```

---

## Files Output Structure (TypeScript, `--generate all`)

```
<output>/
├── types.ts                     # Protocol + type definitions
├── rpc.ts                       # RPC client interface
├── transport.ts                 # WebSocket/browser transport (omitted for --transport none)
├── index.ts                     # Top-level re-exports
├── package.json                 # Written once; user-owned thereafter
├── tsconfig.json                # TypeScript config
├── test/
│   ├── smoke.test.ts            # Schema walk smoke test
│   └── bidir-smoke.test.ts      # Bidirectional smoke test (if applicable)
├── <namespace>/
│   ├── types.ts
│   ├── client.ts
│   └── index.ts
└── .codegen-metadata.json       # Toolchain info + file hashes
```

Cache manifest: `~/.cache/plexus-codegen/hub-codegen/typescript/<backend>/manifest.json`

---

## Version Management

- **hub-codegen version** — `env!("CARGO_PKG_VERSION")` via `HUB_CODEGEN_VERSION` constant (`src/lib.rs`). Included in .codegen-metadata.json.
- **package.json version** — `"0.0.0-<16-char-hash>"` where hash is computed from generated code files (excluding package.json and metadata). Changes only when code changes.
- **IR version** — Must be `"2.0"` (enforced in TypeScript generator).
- **Cache manifest version** — `"2.0"` in `CodeCacheManifest`.

---

## Error Handling

- **IR parsing errors** — Bail if JSON invalid or IR version not `"2.0"`
- **File I/O errors** — Propagate via `anyhow::Result`
- **Feature gating** — Bail if target language not compiled in
- **Schema gaps** — Emit warnings for `RefUnknown` types; `RefAny` (intentionally dynamic) does not warn

---

## Integration with synapse-cc

synapse-cc drives hub-codegen via json mode:

1. Calls `hub-codegen --output-format json`
2. Parses stdout as `CodegenOutput`
3. Performs its own three-way merge (`SynapseCC/Merge.hs`)
4. Adds dependencies via `bun add` / `bun add -D`
5. Writes IR and cache manifests to `synapse.lock`
6. Runs build, test, and other language tools

This keeps hub-codegen a pure code generator while synapse-cc owns merge, cache, and write decisions.

---

## Feature Flags

```toml
[features]
default = ["typescript"]
typescript = []
rust = []
all = ["typescript", "rust"]
```
