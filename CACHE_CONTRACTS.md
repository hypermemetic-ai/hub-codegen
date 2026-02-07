# Cache System Integration Contracts

This document defines the shared interfaces, data structures, and file formats that all cache components must follow.

## Core Principle

**All cache components communicate exclusively through JSON files on disk.**

No RPC, no shared libraries, no FFI. This enables:
- ✅ Language independence (Haskell ↔ Rust)
- ✅ Debuggability (inspect cache with `jq`, `cat`)
- ✅ Versioning (JSON schema can evolve)
- ✅ Tooling (standard tools work)

## What's New in Hash System V2

**Version 2.0 introduces granular hash fields for fine-grained cache invalidation:**

| Feature | V1 | V2 |
|---------|----|----|
| **Hash Fields** | `hash` only (composite) | `hash`, `self_hash`, `children_hash` |
| **Invalidation Granularity** | All-or-nothing per plugin | Method vs. children separately |
| **Cache Hit Rate** | Lower (overly conservative) | Higher (precise matching) |
| **Build Speed** | Slower (unnecessary regeneration) | Faster (skip unchanged parts) |
| **Debugging** | "Hash mismatch" (unclear why) | "Methods changed, children cached" (clear) |
| **Backward Compatibility** | N/A | ✅ V2 readers support V1 caches |

**Key V2 Benefits:**
- 🎯 **Precise invalidation**: Only regenerate what actually changed
- ⚡ **Faster builds**: 50% speed improvement when only methods OR children change
- 🔍 **Better debugging**: Know exactly what changed in each plugin
- 🔄 **Backward compatible**: V1 caches work with V2 readers
- 👥 **Team scalability**: Parallel development on methods vs. children

**When to Use V2:**
- ✅ Large codebases with many plugins
- ✅ Frequent schema changes during development
- ✅ Team workflows with independent method/child development
- ✅ CI/CD pipelines that benefit from precise cache hits

**Migration Path:**
- Phase 1: Add optional V2 fields to cache structures
- Phase 2: Populate V2 fields when substrate provides them
- Phase 3: Use V2 fields for invalidation decisions
- Phase 4: Measure performance improvements

See [Section 11](#11-backward-compatibility-strategy) for full migration details.

---

## 1. Hash Sources (USE EXISTING PLEXUS HASHES!)

**IMPORTANT: Plexus RPC already provides content-based hashes. Use them directly.**

### Hash System V2: Granular Hash Fields

**IMPORTANT: Hash System V2 introduces granular hash tracking for fine-grained cache invalidation.**

Plexus schemas include hashes at multiple levels with V2 enhancements:

```rust
// From plexus-core/src/plexus/schema.rs
pub struct PluginSchema {
    pub namespace: String,
    pub version: String,
    pub description: String,
    pub hash: String,         // ← Composite hash (rollup of self + children)
    pub self_hash: String,    // ← V2: Hash of own methods only
    pub children_hash: String, // ← V2: Hash of children only
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

**Hash Field Semantics:**
- `hash`: Composite hash of entire plugin (self + children) - **backward compatible**
- `self_hash`: Hash of plugin's own methods only (excludes children)
- `children_hash`: Hash of all child plugins (excludes methods)

### Hash Hierarchy (V2)

```
Global Hash (plexus_hash = "194b22dbccdb5ea6")
├── cone.hash = hash_of(cone.self_hash + cone.children_hash)
│   ├── cone.self_hash = hash_of([cone.chat.hash, cone.create.hash, ...])
│   │   ├── cone.chat.hash = hash_of(signature + description)
│   │   └── cone.create.hash = hash_of(signature + description)
│   └── cone.children_hash = hash_of([child1.hash, child2.hash, ...])
├── arbor.hash = hash_of(arbor.self_hash + arbor.children_hash)
│   ├── arbor.self_hash = hash_of([arbor.tree_get.hash, ...])
│   └── arbor.children_hash = "0" (no children)
└── ...
```

**Benefits of Granular Hashing:**
- **Precise invalidation**: Change to plugin methods → only `self_hash` changes
- **Child isolation**: Change to child plugin → only `children_hash` changes
- **Backward compatible**: `hash` field still provides composite validation

### How to Use V2 Hash Fields

**For Schema Cache (with V2 granular hashing):**
```rust
// Schema fetched from substrate already includes all hash fields
let schema: PluginSchema = fetch_schema("cone");
let schema_hash = schema.hash;         // Composite hash
let self_hash = schema.self_hash;      // Methods-only hash
let children_hash = schema.children_hash; // Children-only hash

// Store in cache with V2 fields
let cache_entry = SchemaCacheEntry {
    version: "2.0",  // ← V2 format
    plugin: "cone",
    schema_hash,      // Backward compatible composite
    self_hash,        // V2: For fine-grained invalidation
    children_hash,    // V2: For fine-grained invalidation
    fetched_at: now(),
    substrate_hash: global_plexus_hash,
    schema,
};
```

**When to Use Which Hash:**

| Use Case | Hash Field | Reason |
|----------|------------|--------|
| **Backward compatibility** | `hash` | Full plugin validation |
| **Method changes only** | `self_hash` | Skip child re-validation |
| **Child changes only** | `children_hash` | Skip method re-processing |
| **Fine-grained cache** | Both `self_hash` + `children_hash` | Optimal invalidation |

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
  "version": "2.0",           // Cache entry format version (V2 with granular hashes)
  "plugin": string,           // Plugin name (e.g., "cone")
  "schemaHash": string,       // ← From PluginSchema.hash (composite, backward compatible)
  "selfHash": string,         // ← V2: From PluginSchema.self_hash (methods only)
  "childrenHash": string,     // ← V2: From PluginSchema.children_hash (children only)
  "fetchedAt": string,        // ISO 8601 timestamp
  "substrateHash": string,    // Global plexus_hash at fetch time
  "schema": PluginSchema      // The actual schema from substrate
}
```

**Version Migration:**
- `version: "1.0"`: Uses only `schemaHash` (composite)
- `version: "2.0"`: Adds `selfHash` and `childrenHash` for fine-grained invalidation
- Readers MUST support both versions for backward compatibility

**Example (V2 with granular hashes):**
```json
{
  "version": "2.0",
  "plugin": "cone",
  "schemaHash": "a1b2c3d4e5f6g7h8",
  "selfHash": "abc123methods",
  "childrenHash": "0000000000000000",
  "fetchedAt": "2026-02-06T01:30:00Z",
  "substrateHash": "194b22dbccdb5ea6",
  "schema": {
    "namespace": "cone",
    "version": "1.0.0",
    "description": "LLM cone with persistent conversation context",
    "hash": "a1b2c3d4e5f6g7h8",
    "self_hash": "abc123methods",
    "children_hash": "0000000000000000",
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

**Example (V2 with children):**
```json
{
  "version": "2.0",
  "plugin": "arbor",
  "schemaHash": "xyz789composite",
  "selfHash": "xyz111methods",
  "childrenHash": "xyz222children",
  "fetchedAt": "2026-02-06T01:30:00Z",
  "substrateHash": "194b22dbccdb5ea6",
  "schema": {
    "namespace": "arbor",
    "version": "1.0.0",
    "description": "Tree data structure plugin",
    "hash": "xyz789composite",
    "self_hash": "xyz111methods",
    "children_hash": "xyz222children",
    "methods": [ /* ... */ ],
    "children": [
      {
        "namespace": "arbor.leaf",
        "description": "Leaf node operations",
        "hash": "child1hash"
      }
    ]
  }
}
```

**PluginSchema Type** (matches substrate's output with V2 hash fields):
```typescript
interface PluginSchema {
  psName: string;
  psVersion: string;
  psDescription: string;
  psHash: string;           // Composite hash (backward compatible)
  psSelfHash: string;       // V2: Methods-only hash
  psChildrenHash: string;   // V2: Children-only hash
  psMethods: MethodSchema[];
  psChildren?: ChildSchema[] | null;
}

interface MethodSchema {
  msName: string;
  msDescription: string;
  msHash: string;           // Method-level hash
  msParameters: ParameterSchema;
  msReturns: Schema;
  msStreaming?: boolean;
}

interface ChildSchema {
  csNamespace: string;
  csDescription: string;
  csHash: string;           // Child plugin hash
}
```

**SchemaCacheEntry Type** (V2 with granular hashes):
```typescript
interface SchemaCacheEntry {
  version: "1.0" | "2.0";
  plugin: string;
  schemaHash: string;        // Composite (always present)
  selfHash?: string;         // V2: Optional for backward compat
  childrenHash?: string;     // V2: Optional for backward compat
  fetchedAt: string;         // ISO 8601
  substrateHash: string;     // Global plexus hash
  schema: PluginSchema;
}

// Helper functions for V2 support
function getSelfHash(entry: SchemaCacheEntry): string {
  return entry.selfHash ?? entry.schemaHash;
}

function getChildrenHash(entry: SchemaCacheEntry): string {
  return entry.childrenHash ?? entry.schemaHash;
}

function hasGranularHashes(entry: SchemaCacheEntry): boolean {
  return entry.selfHash !== undefined && entry.childrenHash !== undefined;
}

// Example usage in cache invalidation
function shouldInvalidate(
  cached: SchemaCacheEntry,
  fresh: PluginSchema,
  checkType: "methods" | "children" | "full"
): boolean {
  if (!hasGranularHashes(cached)) {
    // V1 fallback: use composite hash
    return fresh.psHash !== cached.schemaHash;
  }

  // V2: Use granular hashes
  switch (checkType) {
    case "methods":
      return fresh.psSelfHash !== cached.selfHash;
    case "children":
      return fresh.psChildrenHash !== cached.childrenHash;
    case "full":
      return fresh.psHash !== cached.schemaHash;
  }
}
```

---

## 4. Schema Cache Manifest Format

**File:** `$CACHE_ROOT/synapse/schemas/manifest.json`

```typescript
{
  "version": "2.0",                // Manifest format version (V2 with granular hashes)
  "substrateHash": string,         // Global substrate hash
  "updatedAt": string,             // ISO 8601 timestamp
  "plugins": {
    [pluginName: string]: {
      "schemaHash": string,        // Composite hash (backward compatible)
      "selfHash": string,          // V2: Methods-only hash
      "childrenHash": string,      // V2: Children-only hash
      "cachedAt": string           // When it was cached
    }
  }
}
```

**Example (V2):**
```json
{
  "version": "2.0",
  "substrateHash": "194b22dbccdb5ea6",
  "updatedAt": "2026-02-06T01:30:00Z",
  "plugins": {
    "cone": {
      "schemaHash": "abc123composite",
      "selfHash": "abc111methods",
      "childrenHash": "0000000000000000",
      "cachedAt": "2026-02-06T01:30:00Z"
    },
    "arbor": {
      "schemaHash": "def456composite",
      "selfHash": "def111methods",
      "childrenHash": "def222children",
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

**V2 Fine-Grained Invalidation (per plugin):**

Use granular hash fields to determine what actually changed:

| Changed Hash | What Changed | Action Required |
|--------------|--------------|-----------------|
| `self_hash` only | Plugin methods modified | Regenerate method bindings only |
| `children_hash` only | Child plugins modified | Re-fetch child schemas only |
| Both changed | Methods + children | Full plugin regeneration |
| `schemaHash` changed | Catch-all (backward compat) | Full plugin regeneration |

**Example (V1 - Simple Invalidation):**
```rust
// V1: Simple hash check (backward compatible)
let current_global = fetch_substrate_hash();  // "194b22dbccdb5ea6"
if cached_manifest.substrate_hash != current_global {
    invalidate_all_schemas();
    return;
}

// Check individual plugins with composite hash
for plugin in plugins {
    let fresh_schema = fetch_schema(plugin);
    let cached = load_cached_schema(plugin);

    if fresh_schema.hash != cached.schema_hash {
        invalidate_schema(plugin);
        fetch_and_cache(plugin);
    }
}
```

**Example (V2 - Fine-Grained Invalidation):**
```rust
// V2: Granular hash checking for optimal invalidation
let current_global = fetch_substrate_hash();
if cached_manifest.substrate_hash != current_global {
    invalidate_all_schemas();
    return;
}

for plugin in plugins {
    let fresh_schema = fetch_schema(plugin);
    let cached = load_cached_schema(plugin);

    // Check what actually changed
    let methods_changed = fresh_schema.self_hash != cached.self_hash;
    let children_changed = fresh_schema.children_hash != cached.children_hash;

    match (methods_changed, children_changed) {
        (true, true) => {
            // Both changed - full regeneration
            invalidate_schema(plugin);
            invalidate_ir(plugin);
            fetch_and_cache(plugin);
        }
        (true, false) => {
            // Only methods changed - skip child processing
            invalidate_methods_only(plugin);
            fetch_and_cache_methods(plugin);
        }
        (false, true) => {
            // Only children changed - skip method processing
            invalidate_children_only(plugin);
            fetch_and_cache_children(plugin);
        }
        (false, false) => {
            // Nothing changed - cache hit
            continue;
        }
    }
}
```

**Benefits of V2 Invalidation:**
- **Faster builds**: Skip unnecessary work when only methods or children change
- **Precise cache hits**: More granular cache key matching
- **Better debugging**: Know exactly what changed in a plugin

### When to Use self_hash vs children_hash vs hash

**Decision Tree for Cache Validation:**

```
Is this a schema cache check?
├─ Yes: Use composite hash first (backward compat)
│   └─ Need fine-grained invalidation?
│       ├─ Methods changed? → Check self_hash
│       └─ Children changed? → Check children_hash
└─ No: Is this IR generation?
    ├─ Building method bindings? → Use self_hash
    └─ Resolving child dependencies? → Use children_hash
```

**Use Cases by Hash Field:**

| Operation | Use `hash` | Use `self_hash` | Use `children_hash` |
|-----------|-----------|----------------|-------------------|
| Full schema validation | ✅ Primary | ❌ | ❌ |
| Method binding generation | ⚠️ Fallback | ✅ Primary | ❌ |
| Child dependency resolution | ⚠️ Fallback | ❌ | ✅ Primary |
| Cache key for IR | ✅ Composite | ✅ Fine-grained | ✅ Fine-grained |
| Backward compatibility | ✅ Required | ⚠️ V2 only | ⚠️ V2 only |

**Example: Optimal Cache Strategy**
```rust
// Prefer fine-grained hashing when available
fn compute_cache_key(schema: &PluginSchema, operation: CacheOp) -> String {
    match operation {
        CacheOp::MethodBindings if has_v2_hashes(schema) => {
            // Use self_hash for method-only operations
            schema.self_hash.clone()
        }
        CacheOp::ChildResolution if has_v2_hashes(schema) => {
            // Use children_hash for child-only operations
            schema.children_hash.clone()
        }
        _ => {
            // Fallback to composite hash for backward compat
            schema.hash.clone()
        }
    }
}

fn has_v2_hashes(schema: &PluginSchema) -> bool {
    !schema.self_hash.is_empty() && !schema.children_hash.is_empty()
}
```

### IR Cache Invalidation

**V2: Fine-Grained IR Invalidation**

**Invalidate plugin IR if:**
- Source `selfHash` changed (methods modified)
- Source `childrenHash` changed (children modified)
- Source `schemaHash` changed (backward compat catch-all)
- Any dependency's `irHash` changed (transitive)

**Example (V1 - Simple):**
```rust
// cone depends on arbor
// If arbor's schema changes:
if arbor_schema.hash != cached_arbor_schema.hash {
    invalidate_ir("arbor");
    invalidate_ir("cone");  // Transitive!
}
```

**Example (V2 - Fine-Grained):**
```rust
// V2: More precise invalidation using granular hashes
let arbor_fresh = fetch_schema("arbor");
let arbor_cached = load_cached_schema("arbor");
let cone_cached = load_cached_schema("cone");

// Check if arbor's methods changed (affects arbor IR only)
if arbor_fresh.self_hash != arbor_cached.self_hash {
    invalidate_ir("arbor");
    // cone only depends on arbor's types, not methods
    // So we DON'T need to invalidate cone unless it uses arbor methods
}

// Check if arbor's children changed (may affect cone)
if arbor_fresh.children_hash != arbor_cached.children_hash {
    invalidate_ir("arbor");
    // Check if cone depends on arbor's children
    if cone_depends_on_arbor_children(cone_cached) {
        invalidate_ir("cone");  // Transitive!
    }
}
```

**V2 Dependency Analysis:**
```rust
// Fine-grained dependency tracking with V2 hashes
struct PluginDependency {
    plugin: String,
    depends_on_methods: bool,    // Uses methods from dependency
    depends_on_children: bool,   // Uses children from dependency
}

fn should_invalidate_dependent(
    dependency: &PluginDependency,
    dep_self_changed: bool,
    dep_children_changed: bool,
) -> bool {
    (dep_self_changed && dependency.depends_on_methods) ||
    (dep_children_changed && dependency.depends_on_children)
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

## 10. Hash System V2 Real-World Examples

### Example 1: Method Signature Change (Only self_hash Changes)

**Scenario:** Developer adds a new parameter to `cone.chat` method.

**V1 Behavior (without granular hashes):**
```rust
// cone.hash changes → Full invalidation
invalidate_schema_cache("cone");
invalidate_ir_cache("cone");
invalidate_code_cache("cone");
// Even though children didn't change!
```

**V2 Behavior (with granular hashes):**
```rust
// Only cone.self_hash changes, children_hash stays same
if fresh.self_hash != cached.self_hash {
    // Only regenerate method bindings
    invalidate_method_bindings("cone");
    regenerate_methods("cone");
    // Skip child processing - cache hit!
    reuse_children_from_cache("cone");
}
// Result: 50% faster regeneration
```

### Example 2: Child Plugin Change (Only children_hash Changes)

**Scenario:** A child plugin under `arbor` is modified (e.g., `arbor.leaf`).

**V1 Behavior:**
```rust
// arbor.hash changes → Full invalidation
invalidate_schema_cache("arbor");
invalidate_ir_cache("arbor");
invalidate_code_cache("arbor");
// Even though arbor's own methods didn't change!
```

**V2 Behavior:**
```rust
// Only arbor.children_hash changes, self_hash stays same
if fresh.children_hash != cached.children_hash {
    // Only update child references
    invalidate_child_bindings("arbor");
    regenerate_child_refs("arbor");
    // Reuse method bindings - cache hit!
    reuse_methods_from_cache("arbor");
}
// Result: Methods already compiled, just update child refs
```

### Example 3: Independent Plugin Development

**Scenario:** Team A works on `cone` methods, Team B works on `cone` children.

**V1 Behavior:**
```rust
// Both teams' changes invalidate entire cone plugin
// Frequent cache misses, slow iteration
```

**V2 Behavior:**
```rust
// Team A changes methods
if fresh.self_hash != cached.self_hash {
    regenerate_methods("cone");  // Only Team A's work
}

// Team B changes children (different PR)
if fresh.children_hash != cached.children_hash {
    regenerate_children("cone");  // Only Team B's work
}

// No conflicts, both can work independently with cache hits
```

### Example 4: Debugging Cache Misses

**V1 Behavior:**
```bash
$ hub-codegen --debug
Cache miss for 'cone' - hash mismatch
  Expected: a1b2c3d4e5f6g7h8
  Got:      x9y8z7w6v5u4t3s2
# Can't tell WHAT changed!
```

**V2 Behavior:**
```bash
$ hub-codegen --debug
Cache analysis for 'cone':
  Composite hash:  MISMATCH ❌
  Methods hash:    MATCH ✅ (abc111methods)
  Children hash:   MISMATCH ❌ (changed: xyz000 → xyz999)

Conclusion: Children modified, methods unchanged
Action: Reusing method cache, regenerating child bindings only
# Clear diagnostic information!
```

---

## 11. Backward Compatibility Strategy

### Reading V1 and V2 Cache Entries

All cache readers MUST support both formats:

```rust
#[derive(Deserialize)]
struct SchemaCacheEntry {
    version: String,
    plugin: String,
    schema_hash: String,

    // V2 fields (optional for backward compat)
    #[serde(default)]
    self_hash: Option<String>,
    #[serde(default)]
    children_hash: Option<String>,

    fetched_at: String,
    substrate_hash: String,
    schema: PluginSchema,
}

impl SchemaCacheEntry {
    /// Get self hash, falling back to composite hash for V1
    fn get_self_hash(&self) -> &str {
        self.self_hash.as_deref().unwrap_or(&self.schema_hash)
    }

    /// Get children hash, falling back to composite hash for V1
    fn get_children_hash(&self) -> &str {
        self.children_hash.as_deref().unwrap_or(&self.schema_hash)
    }

    /// Check if this entry has V2 granular hashes
    fn has_granular_hashes(&self) -> bool {
        self.self_hash.is_some() && self.children_hash.is_some()
    }
}
```

**Migration Strategy:**
1. **Read**: Accept both V1 and V2 formats
2. **Write**: Always write V2 format (when source provides it)
3. **Validate**: Use granular hashes if available, fall back to composite

**Haskell Example:**
```haskell
data SchemaCacheEntry = SchemaCacheEntry
  { sceVersion :: Text
  , scePlugin :: Text
  , sceSchemaHash :: Text
  , sceSelfHash :: Maybe Text      -- V2 only
  , sceChildrenHash :: Maybe Text  -- V2 only
  , sceFetchedAt :: UTCTime
  , sceSubstrateHash :: Text
  , sceSchema :: PluginSchema
  } deriving (Generic, FromJSON, ToJSON)

getSelfHash :: SchemaCacheEntry -> Text
getSelfHash entry = fromMaybe (sceSchemaHash entry) (sceSelfHash entry)

getChildrenHash :: SchemaCacheEntry -> Text
getChildrenHash entry = fromMaybe (sceSchemaHash entry) (sceChildrenHash entry)
```

### Writing V2 Entries

Always include all three hash fields for maximum compatibility:

```typescript
// TypeScript cache writer
function writeSchemaCacheEntry(
  plugin: string,
  schema: PluginSchema
): SchemaCacheEntry {
  return {
    version: "2.0",
    plugin,
    schemaHash: schema.hash,           // Composite (always present)
    selfHash: schema.self_hash,        // V2 granular
    childrenHash: schema.children_hash, // V2 granular
    fetchedAt: new Date().toISOString(),
    substrateHash: await fetchSubstrateHash(),
    schema,
  };
}
```

---

## 12. Shared Type Definitions

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

## 13. Error Handling

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

## 14. Testing Contract

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

### Test 7: V2 Fine-Grained Invalidation (V2 Only)
```
Given: Cache with V2 granular hashes
When: Only plugin methods change (self_hash changes)
Then: Only method bindings regenerated, children reused from cache
```

### Test 8: V2 Backward Compatibility
```
Given: Mix of V1 and V2 cache entries
When: Read cache entries
Then: Both formats read successfully, V1 falls back to composite hash
```

### Test 9: V2 Granular Cache Hit
```
Given: Plugin with children
When: Change only methods (self_hash changes)
Then: Method cache miss, children cache hit
```

---

## 15. Versioning Strategy

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

## 16. Performance Targets

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

## 17. Summary Checklist

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
- [ ] V2 granular hash support implemented
- [ ] V1 backward compatibility maintained
- [ ] Fine-grained invalidation logic working

---

## 18. Integration Points Summary

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
- Contract: All hashes are lowercase hex (16 chars for Plexus hashes)
- Contract: All timestamps are ISO 8601 UTC
- Contract: V2 entries include all three hash fields (hash, self_hash, children_hash)
- Contract: V1 readers MUST support reading V2 entries

---

## 19. Hash System V2 Migration Guide

### For Cache Writers (Synapse, hub-codegen)

**Step 1: Detect V2 Support**
```rust
fn detect_hash_version(schema: &PluginSchema) -> HashVersion {
    if !schema.self_hash.is_empty() && !schema.children_hash.is_empty() {
        HashVersion::V2
    } else {
        HashVersion::V1
    }
}
```

**Step 2: Write Both Versions**
```rust
// Always write all three fields when available
let cache_entry = json!({
    "version": "2.0",
    "schemaHash": schema.hash,
    "selfHash": schema.self_hash.or(&schema.hash),      // Fallback
    "childrenHash": schema.children_hash.or(&schema.hash), // Fallback
    // ... other fields
});
```

**Step 3: Read with Fallback**
```rust
let self_hash = entry.self_hash
    .as_ref()
    .unwrap_or(&entry.schema_hash);
```

### For Cache Readers

**Support Both Formats:**
```typescript
interface SchemaCacheEntry {
  version: "1.0" | "2.0";
  schemaHash: string;
  selfHash?: string;      // Optional for V1 compat
  childrenHash?: string;  // Optional for V1 compat
  // ... other fields
}

function getSelfHash(entry: SchemaCacheEntry): string {
  return entry.selfHash ?? entry.schemaHash;
}

function getChildrenHash(entry: SchemaCacheEntry): string {
  return entry.childrenHash ?? entry.schemaHash;
}
```

### Migration Timeline

1. **Phase 1**: Add V2 fields to all cache structures (optional)
2. **Phase 2**: Update writers to populate V2 fields when available
3. **Phase 3**: Update readers to use V2 fields preferentially
4. **Phase 4**: Enable fine-grained invalidation logic
5. **Phase 5**: Monitor cache hit rates and performance improvements

**No Breaking Changes**: V1 caches continue to work throughout migration.

---

This contract document is the **source of truth** for all cache implementations.
Any deviation must be documented and approved.
