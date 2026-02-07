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

## 1. Hash Algorithm (CRITICAL - Must Match!)

**All components MUST use the same hashing algorithm:**

```
Algorithm: SHA-256
Encoding: Hex (lowercase)
Input: Canonical JSON (sorted keys, no whitespace)
```

### Canonical JSON Rules

Before hashing any JSON:
1. Sort all object keys alphabetically
2. Remove all whitespace
3. Use consistent number formatting (no trailing zeros)
4. Encode as UTF-8 bytes

### Reference Implementation

**Rust:**
```rust
use sha2::{Sha256, Digest};
use serde_json::Value;

pub fn canonical_hash(value: &Value) -> String {
    // Convert to canonical form
    let canonical = to_canonical_json(value);

    // Hash
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let result = hasher.finalize();

    // Hex encode (lowercase)
    hex::encode(result)
}

fn to_canonical_json(value: &Value) -> String {
    // Sort keys, remove whitespace
    serde_json::to_string(&canonicalize(value)).unwrap()
}

fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted: Vec<_> = map.iter().collect();
            sorted.sort_by_key(|(k, _)| *k);
            Value::Object(
                sorted.into_iter()
                    .map(|(k, v)| (k.clone(), canonicalize(v)))
                    .collect()
            )
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(canonicalize).collect())
        }
        other => other.clone(),
    }
}
```

**Haskell:**
```haskell
import Crypto.Hash (hash, SHA256(..))
import Data.Aeson (Value, encode)
import qualified Data.ByteString.Base16 as B16
import qualified Data.ByteString.Lazy as LBS
import qualified Data.Text as T
import qualified Data.Text.Encoding as TE

canonicalHash :: Value -> T.Text
canonicalHash val =
  let canonical = canonicalizeJSON val
      jsonBytes = LBS.toStrict $ encode canonical
      hashBytes = hash jsonBytes :: Digest SHA256
      hexEncoded = B16.encode $ convert hashBytes
  in TE.decodeUtf8 hexEncoded

canonicalizeJSON :: Value -> Value
canonicalizeJSON (Object obj) =
  Object $ fromList $ sort $ map (\(k, v) -> (k, canonicalizeJSON v)) $ toList obj
canonicalizeJSON (Array arr) =
  Array $ fmap canonicalizeJSON arr
canonicalizeJSON other = other
```

### Test Vectors

All implementations MUST pass these tests:

```json
// Input 1
{"b": 2, "a": 1}

// Canonical
{"a":1,"b":2}

// Hash (SHA-256, hex)
"5feceb66ffc86f38d952786c6d696c79c2dbc239dd4e91b46729d73a27fb57e9"
```

```json
// Input 2
{"plugins": ["cone", "arbor"], "version": "2.0"}

// Canonical
{"plugins":["cone","arbor"],"version":"2.0"}

// Hash
"d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35"
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
  "schemaHash": string,       // SHA-256 hash of schema content
  "fetchedAt": string,        // ISO 8601 timestamp
  "substrateHash": string,    // Global substrate hash at fetch time
  "schema": PluginSchema      // The actual schema (see below)
}
```

**Example:**
```json
{
  "version": "1.0",
  "plugin": "cone",
  "schemaHash": "abc123def456...",
  "fetchedAt": "2026-02-06T01:30:00Z",
  "substrateHash": "194b22dbccdb5ea6",
  "schema": {
    "psName": "cone",
    "psVersion": "1.0.0",
    "psDescription": "LLM cone with persistent conversation context",
    "psMethods": [...],
    "psChildren": null
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
- Global `substrateHash` changed

**Invalidate SINGLE schema if:**
- Plugin's `schemaHash` changed (compared to manifest)

### IR Cache Invalidation

**Invalidate plugin IR if:**
- Source `schemaHash` changed
- Any dependency's `irHash` changed (transitive)

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
