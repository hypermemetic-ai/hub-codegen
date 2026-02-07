# Cache System Integration Contracts

This document defines the shared interfaces, data structures, and file formats that all cache components must follow.

## Core Principle

**All cache components communicate exclusively through JSON files on disk.**

No RPC, no shared libraries, no FFI. This enables:
- ✅ Language independence (Haskell ↔ Rust)
- ✅ Debuggability (inspect cache with `jq`, `cat`)
- ✅ Versioning (JSON schema can evolve)
- ✅ Tooling (standard tools work)

---

## 1. Hash Sources (USE EXISTING PLEXUS HASHES!)

**IMPORTANT: Plexus RPC already provides content-based hashes. Use them directly.**

### Existing Hash Fields in Plexus

Plexus schemas include hashes at multiple levels:

```rust
// From plexus-core/src/plexus/schema.rs
pub struct PluginSchema {
    pub namespace: String,
    pub version: String,
    pub description: String,
    pub hash: String,  // ← Plugin-level hash (rollup of all methods)
    pub methods: Vec<MethodSchema>,
    pub children: Option<Vec<ChildSummary>>,
}

pub struct MethodSchema {
    pub name: String,
    pub description: String,
    pub hash: String,  // ← Method-level hash (signature + description)
    pub params: Option<schemars::Schema>,
    pub returns: Option<schemars::Schema>,
    pub streaming: bool,
}

pub struct ChildSummary {
    pub namespace: String,
    pub description: String,
    pub hash: String,  // ← Child plugin hash
}
```

### Hash Hierarchy

```
Global Hash (plexus_hash = "194b22dbccdb5ea6")
├── cone.hash = hash_of([cone.chat.hash, cone.create.hash, ...])
│   ├── cone.chat.hash = hash_of(signature + description)
│   └── cone.create.hash = hash_of(signature + description)
├── arbor.hash = hash_of([arbor.tree_get.hash, ...])
└── ...
```

### How to Use Existing Hashes

**For Schema Cache:**
```rust
// Schema fetched from substrate already includes hash
let schema: PluginSchema = fetch_schema("cone");
let schema_hash = schema.hash;  // Use this directly!

// Store in cache
let cache_entry = SchemaCacheEntry {
    version: "1.0",
    plugin: "cone",
    schema_hash,  // ← Use PluginSchema.hash
    fetched_at: now(),
    substrate_hash: global_plexus_hash,  // ← From substrate startup
    schema,
};
```

**For Global Invalidation:**
```rust
// Substrate prints this at startup:
// "Plexus hash: 194b22dbccdb5ea6"
//
// Also available via substrate.hash() method or in StreamMetadata
let global_hash = fetch_global_hash();  // "194b22dbccdb5ea6"

// Invalidate all caches if this changes
if cached_manifest.substrate_hash != global_hash {
    invalidate_all_caches();
}
```

**For IR Cache:**
```rust
// IR hash is hash of the IR content (not schema)
// Compute this for the generated IR fragment
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn hash_ir_fragment(types: &Map<String, TypeDef>, methods: &Map<String, MethodDef>) -> String {
    let mut hasher = DefaultHasher::new();

    // Hash type definitions
    for (name, typedef) in types.iter() {
        name.hash(&mut hasher);
        // Hash typedef structure (simplified)
        serde_json::to_string(typedef).unwrap().hash(&mut hasher);
    }

    // Hash method definitions
    for (name, methoddef) in methods.iter() {
        name.hash(&mut hasher);
        serde_json::to_string(methoddef).unwrap().hash(&mut hasher);
    }

    format!("{:016x}", hasher.finish())
}
```

### Key Differences

| Hash Type | Source | Algorithm | Length | Purpose |
|-----------|--------|-----------|--------|---------|
| **Plugin Schema** | Plexus macro | Rust DefaultHasher | 16 hex chars | Detect schema changes |
| **Method Schema** | Plexus macro | Rust DefaultHasher | 16 hex chars | Detect method changes |
| **Global Plexus** | Runtime rollup | Rust DefaultHasher | 16 hex chars | Detect any change |
| **IR Fragment** | Our implementation | Rust DefaultHasher | 16 hex chars | Detect IR changes |

### Properties of Plexus Hashes

✅ **Content-based** - Same signature → same hash
✅ **Stable across restarts** - Deterministic computation
✅ **Hierarchical** - Plugin hash = hash(method hashes)
✅ **Already computed** - No need to recompute
✅ **Designed for caching** - Explicitly documented purpose

### No Custom Hashing Needed!

**DO NOT implement SHA-256 or canonical JSON hashing.**

Instead:
1. Read `PluginSchema.hash` from substrate responses
2. Read global `plexus_hash` from substrate startup or `substrate.hash()` method
3. Only compute IR hashes using `DefaultHasher` for generated IR content

### How to Fetch Global Plexus Hash

**Option 1: From substrate startup logs**
```bash
$ substrate
...
INFO substrate: Plexus hash: 194b22dbccdb5ea6
```

**Option 2: Via RPC call to substrate.hash()**
```rust
// Synapse can call this method
let response = call_method("substrate", "hash", json!({}));
// Returns: { "value": "194b22dbccdb5ea6" }
```

**Option 3: From any stream response metadata**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "tag": "data",
    "metadata": {
      "provenance": ["substrate", "cone"],
      "plexus_hash": "194b22dbccdb5ea6",
      "timestamp": 1735052400
    },
    "path": "cone.chat",
    "data": { /* ... */ }
  }
}
```

---

## 2. Cache Directory Structure (MANDATORY)

**All cache files MUST follow this exact structure:**

```
$CACHE_ROOT/
├── synapse/
│   ├── schemas/
│   │   ├── manifest.json
│   │   ├── cone.json
│   │   ├── arbor.json
│   │   └── <plugin>.json
│   └── ir/
│       ├── manifest.json
│       ├── cone.json
│       ├── arbor.json
│       └── <plugin>.json
└── hub-codegen/
    ├── rust/
    │   ├── manifest.json
    │   ├── cone/
    │   │   ├── hash.txt
    │   │   └── generated files...
    │   ├── arbor/
    │   └── <plugin>/
    └── typescript/
        └── (same structure)
```

**Default `$CACHE_ROOT`:**
- Linux/macOS: `~/.cache/plexus-codegen/`
- Windows: `%LOCALAPPDATA%\plexus-codegen\cache\`

**Override with:**
- Environment variable: `PLEXUS_CACHE_DIR`
- CLI flag: `--cache-dir <path>`

---

## 3. Schema Cache Entry Format

**File:** `$CACHE_ROOT/synapse/schemas/<plugin>.json`

```typescript
{
  "version": "1.0",           // Cache entry format version
  "plugin": string,           // Plugin name (e.g., "cone")
  "schemaHash": string,       // ← From PluginSchema.hash (Plexus-provided)
  "fetchedAt": string,        // ISO 8601 timestamp
  "substrateHash": string,    // Global plexus_hash at fetch time
  "schema": PluginSchema      // The actual schema from substrate
}
```

**Example:**
```json
{
  "version": "1.0",
  "plugin": "cone",
  "schemaHash": "a1b2c3d4e5f6g7h8",
  "fetchedAt": "2026-02-06T01:30:00Z",
  "substrateHash": "194b22dbccdb5ea6",
  "schema": {
    "namespace": "cone",
    "version": "1.0.0",
    "description": "LLM cone with persistent conversation context",
    "hash": "a1b2c3d4e5f6g7h8",
    "methods": [
      {
        "name": "chat",
        "description": "Stream chat messages",
        "hash": "m123hash",
        "params": { /* JSON Schema */ },
        "returns": { /* JSON Schema */ },
        "streaming": true
      }
    ],
    "children": null
  }
}
```

**PluginSchema Type** (matches substrate's output):
```typescript
interface PluginSchema {
  psName: string;
  psVersion: string;
  psDescription: string;
  psMethods: MethodSchema[];
  psChildren?: ChildSchema[] | null;
}

interface MethodSchema {
  msName: string;
  msDescription: string;
  msParameters: ParameterSchema;
  msReturns: Schema;
  msStreaming?: boolean;
}

interface ChildSchema {
  csNamespace: string;
  csDescription: string;
}
```

---

## 4. Schema Cache Manifest Format

**File:** `$CACHE_ROOT/synapse/schemas/manifest.json`

```typescript
{
  "version": "1.0",                // Manifest format version
  "substrateHash": string,         // Global substrate hash
  "updatedAt": string,             // ISO 8601 timestamp
  "plugins": {
    [pluginName: string]: {
      "schemaHash": string,        // Hash of cached schema
      "cachedAt": string           // When it was cached
    }
  }
}
```

**Example:**
```json
{
  "version": "1.0",
  "substrateHash": "194b22dbccdb5ea6",
  "updatedAt": "2026-02-06T01:30:00Z",
  "plugins": {
    "cone": {
      "schemaHash": "abc123...",
      "cachedAt": "2026-02-06T01:30:00Z"
    },
    "arbor": {
      "schemaHash": "def456...",
      "cachedAt": "2026-02-06T01:30:00Z"
    }
  }
}
```

---

## 5. IR Cache Entry Format

**File:** `$CACHE_ROOT/synapse/ir/<plugin>.json`

```typescript
{
  "version": "1.0",              // Cache entry format version
  "plugin": string,              // Plugin name
  "irHash": string,              // SHA-256 hash of IR content
  "generatedAt": string,         // ISO 8601 timestamp
  "schemaHash": string,          // Hash of source schema
  "dependencies": string[],      // Other plugins this depends on
  "types": {
    [typeName: string]: TypeDef  // Type definitions (from IR)
  },
  "methods": {
    [methodName: string]: MethodDef  // Method definitions (from IR)
  }
}
```

**Example:**
```json
{
  "version": "1.0",
  "plugin": "cone",
  "irHash": "xyz789...",
  "generatedAt": "2026-02-06T01:31:00Z",
  "schemaHash": "abc123...",
  "dependencies": ["arbor"],
  "types": {
    "cone.ChatEvent": {
      "tdName": "ChatEvent",
      "tdNamespace": "cone",
      "tdDescription": "Chat event",
      "tdKind": { /* ... */ }
    }
  },
  "methods": {
    "cone.chat": {
      "mdName": "chat",
      "mdFullPath": "cone.chat",
      "mdNamespace": "cone",
      "mdDescription": "Stream chat messages",
      "mdStreaming": true,
      "mdParams": [...],
      "mdReturns": { /* TypeRef */ }
    }
  }
}
```

**Dependency Detection:**

A plugin depends on another if:
1. Any method parameter type references the other plugin's types
2. Any method return type references the other plugin's types
3. Any type field references the other plugin's types

**Example:**
```json
// cone depends on arbor because:
{
  "methods": {
    "cone.chat": {
      "mdParams": [{
        "pdType": {
          "tag": "RefNamed",
          "contents": {
            "qnNamespace": "arbor",    // References arbor!
            "qnLocalName": "TreeNode"
          }
        }
      }]
    }
  }
}
```

---

## 6. IR Cache Manifest Format

**File:** `$CACHE_ROOT/synapse/ir/manifest.json`

```typescript
{
  "version": "1.0",                // Manifest format version
  "irVersion": string,             // IR format version (e.g., "2.0")
  "updatedAt": string,             // ISO 8601 timestamp
  "plugins": {
    [pluginName: string]: {
      "irHash": string,            // Hash of cached IR
      "schemaHash": string,        // Hash of source schema
      "dependencies": string[],    // Plugin dependencies
      "cachedAt": string           // When it was cached
    }
  }
}
```

**Example:**
```json
{
  "version": "1.0",
  "irVersion": "2.0",
  "updatedAt": "2026-02-06T01:31:00Z",
  "plugins": {
    "cone": {
      "irHash": "xyz789...",
      "schemaHash": "abc123...",
      "dependencies": ["arbor"],
      "cachedAt": "2026-02-06T01:31:00Z"
    },
    "arbor": {
      "irHash": "uvw456...",
      "schemaHash": "def456...",
      "dependencies": [],
      "cachedAt": "2026-02-06T01:31:00Z"
    }
  }
}
```

---

## 7. Code Cache Entry Format

**File:** `$CACHE_ROOT/hub-codegen/<target>/<plugin>/hash.txt`

```
<ir-hash>
```

Just the IR hash as plain text (no JSON overhead).

**Generated files location:**
```
$CACHE_ROOT/hub-codegen/<target>/<plugin>/
├── hash.txt
├── types.rs (or types.ts)
├── methods.rs (or client.ts)
└── ... (other generated files)
```

**Why plain text?**
- Minimal overhead
- Easy to verify: `cat hash.txt`
- Fast to check: single file read

---

## 8. Code Cache Manifest Format

**File:** `$CACHE_ROOT/hub-codegen/<target>/manifest.json`

```typescript
{
  "version": "1.0",              // Manifest format version
  "target": string,              // "rust" or "typescript"
  "updatedAt": string,           // ISO 8601 timestamp
  "plugins": {
    [pluginName: string]: {
      "irHash": string,          // Hash of source IR
      "cachedAt": string         // When it was generated
    }
  }
}
```

**Example:**
```json
{
  "version": "1.0",
  "target": "rust",
  "updatedAt": "2026-02-06T01:32:00Z",
  "plugins": {
    "cone": {
      "irHash": "xyz789...",
      "cachedAt": "2026-02-06T01:32:00Z"
    },
    "arbor": {
      "irHash": "uvw456...",
      "cachedAt": "2026-02-06T01:32:00Z"
    }
  }
}
```

---

## 9. Cache Invalidation Rules

### Schema Cache Invalidation

**Invalidate ALL schemas if:**
- Global `substrateHash` (plexus_hash) changed
- Fetch from substrate: `substrate.hash()` → compare to cached manifest

**Invalidate SINGLE schema if:**
- Plugin's `schemaHash` (PluginSchema.hash) changed
- Fetch schema, compare `schema.hash` to cached `schemaHash`

**Example:**
```rust
// Check if global hash changed
let current_global = fetch_substrate_hash();  // "194b22dbccdb5ea6"
if cached_manifest.substrate_hash != current_global {
    invalidate_all_schemas();
    return;
}

// Check individual plugins
for plugin in plugins {
    let fresh_schema = fetch_schema(plugin);
    let cached = load_cached_schema(plugin);

    if fresh_schema.hash != cached.schema_hash {
        invalidate_schema(plugin);
        fetch_and_cache(plugin);
    }
}
```

### IR Cache Invalidation

**Invalidate plugin IR if:**
- Source `schemaHash` (PluginSchema.hash) changed
- Any dependency's `irHash` changed (transitive)

**Example:**
```rust
// cone depends on arbor
// If arbor's schema changes:
if arbor_schema.hash != cached_arbor_schema.hash {
    invalidate_ir("arbor");
    invalidate_ir("cone");  // Transitive!
}
```

**Algorithm:**
```python
def find_affected_plugins(changed_plugin, manifest):
    affected = {changed_plugin}
    queue = [changed_plugin]

    while queue:
        current = queue.pop(0)

        # Find plugins that depend on current
        for plugin, meta in manifest.plugins.items():
            if current in meta.dependencies and plugin not in affected:
                affected.add(plugin)
                queue.append(plugin)

    return affected
```

### Code Cache Invalidation

**Invalidate plugin code if:**
- Source `irHash` changed

---

## 10. Shared Type Definitions

### TypeRef (from IR)

**Rust:**
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "tag", content = "contents")]
pub enum TypeRef {
    RefNamed(QualifiedName),
    RefPrimitive(String, Option<String>),  // (type, format)
    RefArray(Box<TypeRef>),
    RefOptional(Box<TypeRef>),
    RefAny,
    RefUnknown,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QualifiedName {
    #[serde(rename = "qnNamespace")]
    pub namespace: String,
    #[serde(rename = "qnLocalName")]
    pub local_name: String,
}
```

**Haskell:**
```haskell
data TypeRef
  = RefNamed QualifiedName
  | RefPrimitive Text (Maybe Text)  -- (type, format)
  | RefArray TypeRef
  | RefOptional TypeRef
  | RefAny
  | RefUnknown
  deriving (Eq, Show, Generic, ToJSON, FromJSON)

data QualifiedName = QualifiedName
  { qnNamespace :: Text
  , qnLocalName :: Text
  } deriving (Eq, Show, Generic, ToJSON, FromJSON)
```

**JSON:**
```json
// Named type
{
  "tag": "RefNamed",
  "contents": {
    "qnNamespace": "cone",
    "qnLocalName": "UUID"
  }
}

// Primitive
{
  "tag": "RefPrimitive",
  "contents": ["string", null]
}

// Array
{
  "tag": "RefArray",
  "contents": {
    "tag": "RefPrimitive",
    "contents": ["integer", "int64"]
  }
}
```

---

## 11. Error Handling

All cache operations MUST handle these errors gracefully:

### Cache Read Errors
- **Missing file**: Treat as cache miss, fetch fresh
- **Corrupted JSON**: Log warning, invalidate cache, fetch fresh
- **Version mismatch**: Invalidate cache, fetch fresh
- **Permission denied**: Fail with clear error message

### Cache Write Errors
- **Directory creation failed**: Fail with clear error
- **Write failed**: Continue without caching (warn user)
- **Disk full**: Warn user, disable caching for session

### Example Error Types

**Rust:**
```rust
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Cache file not found: {0}")]
    NotFound(PathBuf),

    #[error("Cache corrupted: {0}")]
    Corrupted(String),

    #[error("Cache version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
```

**Haskell:**
```haskell
data CacheError
  = CacheNotFound FilePath
  | CacheCorrupted Text
  | CacheVersionMismatch { expected :: Text, actual :: Text }
  | CacheIOError IOException
  | CacheJSONError Text
  deriving (Show)
```

---

## 12. Testing Contract

All cache implementations MUST pass these test scenarios:

### Test 1: Fresh Cache (Cold Start)
```
Given: Empty cache directory
When: Generate IR for all plugins
Then: All schemas, IR, and code are cached
```

### Test 2: Full Cache Hit
```
Given: Fully populated cache, no changes
When: Generate IR for all plugins
Then: All data comes from cache, no network requests
```

### Test 3: Single Plugin Change
```
Given: Fully populated cache
When: Change one plugin's schema
Then: Only that plugin (and dependents) are regenerated
```

### Test 4: Dependency Chain
```
Given: cone depends on arbor
When: Change arbor schema
Then: Both arbor and cone are regenerated, others cached
```

### Test 5: Global Hash Change
```
Given: Fully populated cache
When: Substrate global hash changes
Then: All schema cache invalidated, fresh fetch
```

### Test 6: Hash Consistency
```
Given: Same input JSON
When: Hash in Haskell and Rust
Then: Hashes MUST match exactly
```

---

## 13. Versioning Strategy

### Manifest Version Evolution

**v1.0 → v2.0 migration:**
1. Try to read v1.0 format
2. If successful, migrate to v2.0 format
3. If migration fails, invalidate cache
4. Write new v2.0 manifest

**Backward compatibility rule:**
- Never break ability to read old cache
- Always write latest version
- Provide migration path

### Cache Format Changes

**Breaking change process:**
1. Bump version in manifest (e.g., "1.0" → "2.0")
2. Implement migration logic
3. Document change in CHANGELOG
4. Add test for migration

---

## 14. Performance Targets

All cache operations MUST meet these targets:

| Operation | Target | Failure Threshold |
|-----------|--------|-------------------|
| Cache manifest read | < 10ms | > 100ms |
| Single plugin read | < 50ms | > 500ms |
| Cache write (per plugin) | < 100ms | > 1s |
| Hash computation | < 50ms per plugin | > 500ms |
| Dependency resolution | < 100ms | > 1s |

**Measurement:** Use built-in profiling, not manual timing.

---

## 15. Summary Checklist

Before merging any cache implementation, verify:

- [ ] Hash algorithm matches test vectors exactly
- [ ] Cache directory structure follows spec
- [ ] All JSON formats match TypeScript definitions
- [ ] Error handling covers all specified cases
- [ ] All test scenarios pass
- [ ] Performance targets met
- [ ] Version fields present in all manifests
- [ ] Timestamp fields use ISO 8601
- [ ] File permissions set correctly (644 for files, 755 for dirs)
- [ ] Cache invalidation logic correct
- [ ] Dependency tracking implemented
- [ ] Cross-language compatibility tested (Haskell ↔ Rust)

---

## 16. Integration Points Summary

**Synapse Schema Cache → IR Cache:**
- Input: Cached schema entries
- Output: IR cache entries with dependencies
- Contract: Schema hash must match manifest

**Synapse IR Cache → hub-codegen:**
- Input: Merged IR from cache + fresh generation
- Output: Standard IR JSON (version 2.0)
- Contract: IR format matches `src/ir.rs` types

**hub-codegen → Code Cache:**
- Input: IR JSON with plugin grouping
- Output: Generated files + hash.txt
- Contract: Files in `<target>/<plugin>/` directory

**All Components → File System:**
- Contract: All JSON is valid, well-formed
- Contract: All hashes are lowercase hex SHA-256
- Contract: All timestamps are ISO 8601 UTC

---

This contract document is the **source of truth** for all cache implementations.
Any deviation must be documented and approved.
