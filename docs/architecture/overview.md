# hub-codegen Architecture Overview

hub-codegen is a pure, stateless Rust CLI that transforms a Synapse IR JSON document into typed client libraries (TypeScript or Rust). It is designed to be driven by an external orchestrator (synapse-cc) that owns file writes, merge decisions, caching, and dependency management — hub-codegen itself has no side effects in json output mode.

---

## System Context

```
  Synapse Backend (Haskell/Rust plugins)
       │  schema introspection
       ▼
  Synapse IR Builder  ──► ir.json  (irVersion: "2.0")
                                │
                     ┌──────────┘
                     │  hub-codegen --output-format json
                     ▼
              CodegenOutput (stdout JSON)
              ┌──────────────────────────┐
              │ files        (path→text) │
              │ fileHashes   (path→hash) │
              │ dependencies             │
              │ devDependencies          │
              │ warnings                 │
              └──────────────────────────┘
                     │
          ┌──────────┘
          │  synapse-cc (Haskell orchestrator)
          │  • three-way merge (Merge.hs)
          │  • bun add / bun install (Language.hs)
          │  • cache + lock (Cache.hs, Lock.hs)
          │  • build / test / hot-reload (Pipeline.hs)
          ▼
  Generated client on disk
  (types.ts, rpc.ts, transport.ts,
   {ns}/types.ts, {ns}/client.ts, …)
```

Direct use (`--output-format files`) is also supported and performs merge and cache writes internally, but synapse-cc integration always uses json mode.

---

## Key Concepts

**IR** — A language-agnostic JSON document (v2.0) from the Synapse IR Builder. Contains `irTypes`, `irMethods`, and `irPlugins`. `irHash` is a stable content hash (excludes timestamps) used for cache invalidation.

**GenerationResult** — Internal output of any language generator: `files` (path→text), `file_hashes` (path→SHA-256[..16]), `warnings`, `dependencies`, `devDependencies`.

**CodegenOutput** — JSON envelope emitted to stdout in json mode. Same content as `GenerationResult`, camelCase serialized. Consumed by synapse-cc.

**TransportEnv** — Controls the WebSocket implementation in generated TypeScript:
- `ws` (default): Node.js `ws` package, `"types": ["node"]` tsconfig
- `browser`: native `window.WebSocket`, `"lib": ["ES2022", "DOM"]` tsconfig
- `none`: no `transport.ts`; `@plexus/rpc-client` workspace dep assumed

**GenerateSelector** — Artifact subset to produce: `all`, `transport`, `rpc`, `plugins`, `smoke`, `package`. Useful for partial regeneration.

**Three-way merge** — Compares new generated content against the cached baseline hash and the current on-disk hash. Files are only overwritten when the user has not modified them since the last generation run.

---

## Architecture Layers

```
┌────────────────────────────────────────────────┐
│  CLI  src/main.rs                              │
│  clap flags → GenerationOptions                │
│  phase 1: parse IR   phase 2: generate         │
│  phase 3: output (json | files)                │
└───────────────┬────────────────────────────────┘
                │
     ┌──────────┴──────────┐
     │                     │
┌────▼──────────┐   ┌──────▼────────┐
│ TypeScript    │   │ Rust          │
│ Generator     │   │ Generator     │
│ src/generator │   │ src/generator │
│ /typescript/  │   │ /rust/        │
└────┬──────────┘   └──────┬────────┘
     │                     │
     └──────────┬──────────┘
                │  GenerationResult
     ┌──────────▼──────────┐
     │  IR    src/ir.rs    │
     │  Hash  src/hash.rs  │
     │  Merge src/merge.rs │  ← files mode only
     │  Cache src/cache.rs │  ← files mode only
     └─────────────────────┘
```

**`src/ir.rs`** — Rust structs mirroring the IR JSON schema. Deserialised once at startup. Key types: `IR`, `TypeDef`, `TypeKind`, `TypeRef`, `MethodDef`, `VariantDef`.

**`src/generator/typescript/`** — Primary generator. Two layers:
- Layer 1 (protocol): `types.ts`, `rpc.ts`, `transport.ts` — mostly static templates
- Layer 2 (domain): per-namespace `types.ts`, `client.ts`, `index.ts` — dynamic from IR

**`src/generator/rust/`** — Secondary generator (Cargo feature `rust`). Produces `src/types.rs`, `src/client.rs`, `src/{ns}/mod.rs`, `Cargo.toml`. Uses standalone async functions per namespace method.

**`src/hash.rs`** — SHA-256 truncated to 16 hex chars. All file hashes and the plugin composite hash use this function. **The Haskell side in synapse-cc must agree on this algorithm.**

**`src/merge.rs`** — Three-way merge producing `FileStatus` per file (`Unchanged`, `SafeToUpdate`, `NewFile`, `UserModified`). Files mode only.

**`src/cache.rs`** — Reads and writes `~/.cache/plexus-codegen/hub-codegen/` manifest. Files mode only; synapse-cc maintains its own separate cache.

---

## Data Flow

```
ir.json
  │
  ├─ serde_json::from_str → IR struct
  │
  ├─ generate_typescript(&ir, &options)
  │    ├─ types.ts, rpc.ts, transport.ts   (static, transport-variant)
  │    ├─ {ns}/types.ts                    (TypeDef → interface/enum/alias)
  │    ├─ {ns}/client.ts                   (MethodDef → async fn / AsyncGenerator)
  │    └─ index.ts, tsconfig.json, smoke.test.ts
  │
  ├─ compute_file_hashes(&files)
  ├─ compute_plugin_hash(&files)  → package.json version "0.0.0-{hash}"
  │
  └─ output
       ├─ json mode  → CodegenOutput JSON to stdout  (no I/O)
       └─ files mode → merge → write → update cache manifest
```

### IR → TypeScript Mapping

| IR construct | TypeScript output |
|---|---|
| `KindStruct` | `interface` with `?` fields for non-required |
| `KindEnum` | Variant interfaces + union type + type guards |
| `KindAlias` | `type` alias |
| `KindStringEnum` | Union of string literals |
| `KindPrimitive` | `string` / `number` / `boolean` |
| `RefOptional(T)` | `T \| null` |
| `RefArray(T)` | `T[]` |
| `RefAny` | `unknown` (intentional, no warning) |
| `RefUnknown` | `unknown` (schema gap — warning emitted) |
| `md_streaming = true` | `async *method(): AsyncGenerator<T>` |
| `md_streaming = false` | `async method(): Promise<T>` |

---

## Output Modes

**json mode** (`--output-format json`) — default in synapse-cc integration
- No file writes. Emits `CodegenOutput` JSON to stdout.
- synapse-cc parses this, runs its own merge, installs deps, writes `synapse.lock`.
- Fully idempotent and composable.

**files mode** (`--output-format files`) — default for direct CLI use
- Writes generated files to `--output` directory.
- Three-way merge protects user-modified files (`--merge-strategy skip` by default).
- Writes cache manifest to `~/.cache/plexus-codegen/hub-codegen/{target}/{backend}/`.
- Writes starter `package.json` once (user-owned thereafter; excluded from merge).

---

## Extension Points

**New language target** — Add `src/generator/{lang}/mod.rs` with `pub fn generate(ir: &IR) -> Result<GenerationResult>`, gate behind a Cargo feature, add a `CodegenTarget` variant, dispatch in `main.rs`.

**New generate selector** — Add a variant to `GenerateSelector` in `src/generator/mod.rs` and handle it in the TypeScript generator.

**New transport variant** — Add a variant to `TransportEnv`, update `transport.rs` (template), `package.rs` (deps and tsconfig).

**Changing the hash algorithm** — Update `src/hash.rs:compute_hash` and `SynapseCC/Merge.hs:computeFileHash` atomically — both sides must agree.

---

## Where to Go Next

| Topic | Document |
|-------|----------|
| CLI flags and pipeline | `docs/architecture/cli-and-pipeline.md` |
| IR JSON schema and types | `docs/architecture/ir-data-model.md` |
| TypeScript generator detail | `docs/architecture/typescript-generator.md` |
| Caching and three-way merge | `docs/architecture/caching-and-merge.md` |
| Rust generator and test infrastructure | `docs/architecture/rust-generator-and-testing.md` |
