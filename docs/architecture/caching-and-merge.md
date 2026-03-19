# Caching and Merge Architecture

## Overview

Hub-codegen's cache/merge system enables incremental code generation by tracking file hashes through a three-way merge algorithm. The system operates in two distinct output modes: **files mode** (full merge support) and **json mode** (stateless, no file writes). Both modes compute and emit file hashes for downstream caching.

---

## Hash Algorithm

**File**: `src/hash.rs`

**Algorithm**: SHA-256 truncated to 16 hex characters (64 bits).

```rust
pub fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)[..16].to_string()
}
```

- Input: UTF-8 bytes of content
- Output: 16-character lowercase hex string
- Matches the hash format used by synapse-cc (Haskell side must agree)

### Hash Hierarchy

| Function | Input | Purpose |
|----------|-------|---------|
| `compute_file_hash(content)` | Single file content | Granular change detection |
| `compute_file_hashes(files)` | HashMap of files | Batch — returns path → hash map |
| `compute_plugin_hash(files)` | HashMap of files | Composite hash for package.json version; sorts file names first to be order-independent |

The **plugin hash** concatenates `filename:hash\n` for all sorted files, then hashes the result — so identical file sets always produce the same plugin hash regardless of insertion order.

---

## Cache Manifest Format

**File**: `src/cache.rs`

### Directory Location

```
~/.cache/plexus-codegen/hub-codegen/{target}/{backend}/manifest.json
```

- `target`: language target (`"typescript"` or `"rust"`)
- `backend`: from `IR.ir_backend` field (e.g., `"substrate"`)
- Falls back to `USERPROFILE` on Windows

### Schema (Version 2.0)

```rust
pub struct CodeCacheManifest {
    pub version: String,                            // "2.0"
    pub target: String,                             // "typescript" | "rust"
    pub toolchain: ToolchainVersions,
    pub updated_at: String,                         // ISO 8601 timestamp
    pub plugins: HashMap<String, CodePluginCache>,
}

pub struct CodePluginCache {
    pub ir_hash: String,                            // IR content hash
    pub file_hashes: HashMap<String, String>,       // rel_path → file hash
    pub cached_at: String,                          // ISO 8601 timestamp
}

pub struct ToolchainVersions {
    pub synapse_cc: String,
    pub synapse: String,
    pub hub_codegen: String,
}
```

**JSON serialization** uses camelCase field names (`"updatedAt"`, `"irHash"`, `"fileHashes"`, etc.).

### Example manifest.json

```json
{
  "version": "2.0",
  "target": "typescript",
  "toolchain": {
    "synapse-cc": "0.1.0.0",
    "synapse": "0.2.0.0",
    "hub-codegen": "0.1.0"
  },
  "updatedAt": "2026-03-18T10:30:45Z",
  "plugins": {
    "default": {
      "irHash": "abc123def456abcd",
      "fileHashes": {
        "index.ts": "1111111111111111",
        "types.ts": "2222222222222222",
        "rpc.ts":   "3333333333333333"
      },
      "cachedAt": "2026-03-18T10:30:45Z"
    }
  }
}
```

---

## Output Modes and Cache Integration

### Files Mode (`--output-format files`)

The full incremental pipeline (`src/main.rs:196–291`):

1. Generate code → `GenerationResult` with `files` and `file_hashes`
2. Read existing cache manifest (errors silently swallowed; cold start = no manifest)
3. Three-way merge new files against disk using cache as baseline
4. Write safe files, skip user-modified files, print summary to stderr
5. Update and persist cache manifest

**Merge strategy** (`--merge-strategy`):
- `skip` (default) — do not overwrite user-modified files
- `force` — overwrite everything
- `interactive` — not yet implemented

### JSON Mode (`--output-format json`)

Stateless — no file I/O, no cache writes (`src/main.rs:182–195`):

1. Generate code → `GenerationResult`
2. Serialize `CodegenOutput` as JSON to stdout
3. Exit

The JSON payload:
```json
{
  "files":             { "types.ts": "..." },
  "fileHashes":        { "types.ts": "abc1..." },
  "warnings":          [ { "location": "...", "message": "..." } ],
  "hubCodegenVersion": "0.2.0",
  "dependencies":      { "ws": "^8.18.0" },
  "devDependencies":   { "typescript": "^5.0.0" }
}
```

**synapse-cc uses json mode exclusively.** It runs its own three-way merge (Haskell side, `SynapseCC/Merge.hs`) and maintains its own cache at `~/.cache/plexus-codegen/synapse-cc/code/{target}/{backend}/manifest.json`.

---

## Three-Way Merge Algorithm

**File**: `src/merge.rs`

### File Status

```rust
pub enum FileStatus {
    Unchanged,      // cache == current == new  → skip write
    SafeToUpdate,   // cache == current, new differs  → write
    UserModified,   // cache != current  → conflict!
    NewFile,        // not in cache  → write
}
```

### Status Logic

| cached_hash | current_hash | Action |
|-------------|--------------|--------|
| None        | None         | `NewFile` (write) |
| None        | Some(c)      | `Unchanged` if c==new, else `NewFile` |
| Some(_)     | None         | `SafeToUpdate` (recreate deleted file) |
| Some(cached)| Some(current)| If cached==current: `Unchanged` or `SafeToUpdate`; if cached!=current: `UserModified` |

### Merge Dispatch

| Status | `skip` strategy | `force` strategy |
|--------|----------------|-----------------|
| Unchanged | skip write | skip write |
| SafeToUpdate | **write** | **write** |
| NewFile | **write** | **write** |
| UserModified | **skip** (warn) | **write** |

### MergeResult

```rust
pub struct MergeResult {
    pub updated:   Vec<PathBuf>,   // written (safe or force)
    pub skipped:   Vec<PathBuf>,   // NOT written (user modified, skip strategy)
    pub unchanged: Vec<PathBuf>,   // no-op
    pub new:       Vec<PathBuf>,   // newly created
}
```

### Skipped File Hash Preservation

When a file is skipped due to user modification, the **old cached hash is restored** in the updated manifest (`main.rs:271–278`). This ensures the conflict is detected again on the next run, rather than appearing "resolved."

---

## Cache Hit / Miss Logic

**Hit** — manifest exists and `manifest.plugins[plugin].ir_hash == ir.ir_hash`

**Miss** (any of):
- No manifest file
- Plugin not in manifest
- IR hash differs (schema changed)
- Toolchain version differs (manifest kept for conflict detection anyway)

### Workflow Examples

**Cold cache (first run)**:
```
IR ir_hash = "abc123abc123abc1"
→ No manifest
→ Generate all files, write to disk
→ Create manifest: plugins.default.ir_hash = "abc123abc123abc1"
```

**Warm cache (unchanged IR)**:
```
IR ir_hash = "abc123abc123abc1"  (same as cached)
→ All files Unchanged
→ No writes, manifest timestamps updated
```

**IR changed**:
```
IR ir_hash = "def456def456def4"  (differs)
→ All files SafeToUpdate
→ Regenerate and write, update manifest
```

---

## Special Cases

### package.json

Written **only if not present** on disk (`main.rs:207–227`). After first run, the user owns package.json — it is excluded from the three-way merge and not tracked in the cache manifest.

### Dry Run

`--dry-run` prints generated files to stdout without writing to disk and does **not** update the cache manifest.

### Missing Manifest

Read errors are silently swallowed (`.ok()`). The merge proceeds treating all files as `NewFile` or `Unchanged`, and a fresh manifest is created.

---

## Known Limitations

1. **No per-plugin granularity in hub-codegen** — the manifest uses a single `"default"` plugin entry; synapse-cc splits by plugin separately.
2. **No dependency tracking** — if plugin B changes and plugin A depends on it, hub-codegen regenerates everything anyway.
3. **Interactive merge not implemented** — `MergeStrategy::Interactive` bails with a "not yet implemented" error.
4. **Local cache only** — no remote/team cache sharing; CI always rebuilds from scratch.

---

## Testing

**`tests/cache_invalidation_test.rs`** — Integration tests covering:
- Scenario A: Method-only IR change → cache invalidated
- Scenario B: Children-only IR change → cache invalidated
- Scenario C: Both methods and children change
- File deleted from disk → recreated (SafeToUpdate)
- User-created file (not in cache) → not overwritten
- Toolchain version mismatch → manifest updated, files regenerated
- Performance: 1000 hash computations in < 100ms (actual: ~97µs per hash)

**`CACHE_CONTRACTS.md`** — Full JSON schema spec and version contract documentation.

**`INCREMENTAL_CODEGEN.md`** — Design rationale and V2 roadmap (granular `self_hash` / `children_hash` per plugin, not yet implemented).
