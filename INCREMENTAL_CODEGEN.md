# Incremental Codegen Architecture

Design document for smart caching in the Substrate → Synapse → hub-codegen pipeline.

## Problem Statement

Currently, any change to any plugin requires regenerating the entire client:
1. **Synapse** fetches schemas for ALL plugins (slow)
2. **IR generation** processes all plugins (slow)
3. **hub-codegen** regenerates ALL code (slow)

For large systems with many plugins, this becomes prohibitively expensive.

## Proposed Solution: Multi-Level Caching

Cache at THREE levels:
1. **Schema Cache** - Cache fetched schemas per plugin
2. **IR Cache** - Cache generated IR per plugin
3. **Code Cache** - Cache generated code per plugin

## Architecture Overview

```
┌─────────────┐
│  Substrate  │ (serves schemas with hashes)
└──────┬──────┘
       │
       ▼
┌─────────────────────────────────────────┐
│ Schema Cache (per-plugin)               │
│ Key: plugin_name                        │
│ Value: { schema, hash, timestamp }      │
└──────┬──────────────────────────────────┘
       │ (cache miss or hash changed)
       ▼
┌─────────────┐
│   Synapse   │ (builds IR from schemas)
└──────┬──────┘
       │
       ▼
┌─────────────────────────────────────────┐
│ IR Cache (per-plugin)                   │
│ Key: plugin_name                        │
│ Value: { types, methods, hash }         │
└──────┬──────────────────────────────────┘
       │ (cache miss or IR changed)
       ▼
┌─────────────┐
│ hub-codegen │ (generates code from IR)
└──────┬──────┘
       │
       ▼
┌─────────────────────────────────────────┐
│ Code Cache (per-plugin)                 │
│ Key: plugin_name + target_lang          │
│ Value: { generated_files, hash }        │
└─────────────────────────────────────────┘
```

## Level 1: Schema Caching

### Current Behavior
```bash
synapse substrate -i  # Fetches ALL plugin schemas every time
```

### With Schema Cache
```bash
synapse substrate -i --use-cache ~/.cache/synapse/schemas/
```

**Cache Structure:**
```
~/.cache/synapse/schemas/
├── manifest.json          # { "substrate_hash": "194b22db...", "fetched_at": "..." }
├── cone.json             # { "hash": "abc123", "schema": {...}, "fetched_at": "..." }
├── arbor.json            # { "hash": "def456", "schema": {...}, "fetched_at": "..." }
├── hyperforge.json
└── ...
```

**Cache Invalidation:**
1. Substrate serves a global `plexus_hash` (already does: `substrate_hash`)
2. If global hash changed → invalidate all caches
3. Otherwise, fetch schema for each plugin and compare individual hashes
4. Only re-fetch schemas with changed hashes

**Implementation:**

```haskell
-- Synapse/Cache/Schema.hs
data SchemaCache = SchemaCache
  { scManifest :: ManifestCache
  , scPlugins :: Map Text CachedSchema
  }

data CachedSchema = CachedSchema
  { csHash :: Text
  , csSchema :: PluginSchema
  , csFetchedAt :: UTCTime
  }

-- Check if cache is valid
validateCache :: Text -> SchemaCache -> IO (Either InvalidationReason SchemaCache)
validateCache currentGlobalHash cache = do
  if scManifest cache ^. globalHash == currentGlobalHash
    then return (Right cache)
    else return (Left GlobalHashChanged)

-- Fetch schemas with caching
fetchSchemasWithCache :: Path -> SynapseM SchemaCache
fetchSchemasWithCache path = do
  currentHash <- fetchGlobalHash
  existingCache <- loadCacheFromDisk

  case validateCache currentHash existingCache of
    Right validCache -> do
      -- Cache is valid, check individual plugin hashes
      updatedCache <- refreshStalePlugins validCache
      return updatedCache
    Left reason -> do
      -- Cache invalid, fetch everything
      freshCache <- fetchAllSchemas path
      saveCacheToDisk freshCache
      return freshCache
```

**Benefits:**
- Skip network requests for unchanged plugins
- Faster IR generation (especially for large systems)

---

## Level 2: IR Caching

### Current Behavior
```bash
synapse substrate -i  # Builds IR from scratch every time
```

### With IR Cache
```bash
synapse substrate -i --use-cache --cache-dir ~/.cache/synapse/
```

**Cache Structure:**
```
~/.cache/synapse/ir/
├── manifest.json          # { "ir_version": "2.0", "generated_at": "..." }
├── cone.ir.json          # { "hash": "xyz789", "types": {...}, "methods": {...} }
├── arbor.ir.json
├── hyperforge.ir.json
└── ...
```

**Per-Plugin IR:**
Each plugin gets its own IR fragment:
```json
{
  "hash": "plugin_content_hash",
  "irTypes": {
    "cone.ChatEvent": { /* TypeDef */ },
    "cone.ChatResult": { /* TypeDef */ }
  },
  "irMethods": {
    "cone.chat": { /* MethodDef */ },
    "cone.create": { /* MethodDef */ }
  },
  "dependencies": ["arbor", "shared"]  // Other plugins this depends on
}
```

**Cache Invalidation:**
1. Hash each plugin's schema → plugin content hash
2. If plugin hash unchanged → use cached IR
3. If plugin hash changed → regenerate only that plugin's IR
4. If dependency changed → regenerate dependent plugins

**Merging Cached IR:**
```haskell
-- Synapse/Cache/IR.hs
data IRCache = IRCache
  { icPlugins :: Map Text CachedPluginIR
  , icManifest :: IRManifest
  }

data CachedPluginIR = CachedPluginIR
  { cpiHash :: Text
  , cpiTypes :: Map Text TypeDef
  , cpiMethods :: Map Text MethodDef
  , cpiDependencies :: [Text]
  }

-- Merge cached IR fragments into complete IR
mergeIRFragments :: [CachedPluginIR] -> IR
mergeIRFragments fragments = IR
  { irVersion = "2.0"
  , irHash = hashOf $ concatMap cpiHash fragments
  , irTypes = deduplicateTypes $ mconcat (map cpiTypes fragments)
  , irMethods = mconcat (map cpiMethods fragments)
  , irPlugins = buildPluginMap fragments
  }

-- Smart rebuild: only regenerate changed plugins
smartRebuildIR :: [Text] -> IRCache -> SynapseM IR
smartRebuildIR changedPlugins cache = do
  -- Find all plugins affected by changes (transitive dependencies)
  affectedPlugins <- findAffectedPlugins changedPlugins cache

  -- Regenerate only affected plugins
  fresh <- traverse rebuildPlugin affectedPlugins

  -- Merge with cached plugins
  let unchanged = icPlugins cache `Map.difference` Map.fromList [(p, ()) | p <- affectedPlugins]
  let merged = Map.union fresh unchanged

  return $ mergeIRFragments (Map.elems merged)
```

**Dependency Tracking:**
```json
{
  "cone": {
    "hash": "abc123",
    "dependencies": ["arbor"]  // cone.chat uses arbor.TreeNode
  },
  "arbor": {
    "hash": "def456",
    "dependencies": []
  }
}
```

If `arbor` changes:
- Invalidate `arbor` IR
- Invalidate `cone` IR (because it depends on arbor)
- Keep all other plugins cached

**Benefits:**
- Skip IR generation for unchanged plugins
- Smart dependency-based invalidation

---

## Level 3: Code Generation Caching

### Current Behavior
```bash
hub-codegen ir.json -o ./client -t rust  # Regenerates ALL code
```

### With Code Cache
```bash
hub-codegen ir.json -o ./client -t rust --use-cache --cache-dir ~/.cache/hub-codegen/
```

**Cache Structure:**
```
~/.cache/hub-codegen/rust/
├── manifest.json          # { "target": "rust", "ir_hash": "...", "generated_at": "..." }
├── cone/
│   ├── hash.txt          # Content hash of cone IR fragment
│   ├── types.rs
│   └── methods.rs
├── arbor/
│   ├── hash.txt
│   ├── types.rs
│   └── methods.rs
└── shared/
    ├── lib.rs            # Core scaffolding (always regenerated)
    ├── client.rs         # Aggregates all plugins
    └── Cargo.toml
```

**Cache Invalidation:**
1. Hash each plugin's IR fragment
2. If IR hash unchanged → copy from cache
3. If IR hash changed → regenerate only that plugin's code

**Implementation:**

```rust
// hub-codegen/src/cache.rs
use std::collections::HashMap;
use std::path::PathBuf;

pub struct CodeCache {
    cache_dir: PathBuf,
    target: Target,
    plugin_hashes: HashMap<String, String>,
}

impl CodeCache {
    pub fn new(cache_dir: PathBuf, target: Target) -> Self {
        // Load existing cache manifest
        Self {
            cache_dir,
            target,
            plugin_hashes: Self::load_manifest(&cache_dir),
        }
    }

    /// Check if cached code exists and is valid for this plugin
    pub fn is_valid(&self, plugin: &str, ir_hash: &str) -> bool {
        self.plugin_hashes
            .get(plugin)
            .map(|cached_hash| cached_hash == ir_hash)
            .unwrap_or(false)
    }

    /// Get cached code for a plugin
    pub fn get_cached(&self, plugin: &str) -> Option<GeneratedCode> {
        let plugin_dir = self.cache_dir.join(&self.target.to_string()).join(plugin);
        if plugin_dir.exists() {
            Some(GeneratedCode::load_from_dir(&plugin_dir))
        } else {
            None
        }
    }

    /// Save generated code to cache
    pub fn save(&mut self, plugin: &str, ir_hash: &str, code: &GeneratedCode) {
        let plugin_dir = self.cache_dir.join(&self.target.to_string()).join(plugin);
        std::fs::create_dir_all(&plugin_dir).unwrap();

        // Write code files
        code.write_to_dir(&plugin_dir);

        // Write hash
        std::fs::write(plugin_dir.join("hash.txt"), ir_hash).unwrap();

        // Update manifest
        self.plugin_hashes.insert(plugin.to_string(), ir_hash.to_string());
        self.save_manifest();
    }
}

// Smart codegen with caching
pub fn generate_with_cache(
    ir: &IR,
    output: &Path,
    target: Target,
    cache: &mut CodeCache,
) -> Result<()> {
    // Group IR by plugin
    let plugins = group_ir_by_plugin(ir);

    for (plugin_name, plugin_ir) in plugins {
        let ir_hash = hash_plugin_ir(&plugin_ir);

        if cache.is_valid(&plugin_name, &ir_hash) {
            // Use cached code
            let cached = cache.get_cached(&plugin_name).unwrap();
            cached.write_to_dir(&output.join(&plugin_name));
        } else {
            // Generate fresh code
            let generated = generate_plugin_code(&plugin_ir, target)?;
            generated.write_to_dir(&output.join(&plugin_name));

            // Save to cache
            cache.save(&plugin_name, &ir_hash, &generated);
        }
    }

    // Always regenerate core scaffolding (lib.rs, client.rs, etc.)
    generate_core_scaffolding(ir, output, target)?;

    Ok(())
}
```

**Benefits:**
- Skip code generation for unchanged plugins
- Faster builds (especially for large systems)
- Works across different IR versions (hash-based)

---

## Complete Incremental Pipeline

### Without Caching (Current)
```bash
# Change cone plugin in substrate
# Restart substrate

# Full regeneration (SLOW)
synapse substrate -i > ir.json                    # 5-10s
hub-codegen ir.json -o ./client -t rust          # 10-20s
cd ./client && cargo check                        # 30-60s

# Total: 45-90 seconds
```

### With Caching (Proposed)
```bash
# Change cone plugin in substrate
# Restart substrate

# Smart regeneration (FAST)
synapse substrate -i \
  --use-cache \
  --cache-dir ~/.cache/synapse/           # Only fetches cone schema: 0.5s

hub-codegen ir.json -o ./client -t rust \
  --use-cache \
  --cache-dir ~/.cache/hub-codegen/       # Only regenerates cone code: 1s

cd ./client && cargo check                # Incremental compile: 5-10s

# Total: 6-11.5 seconds (6-8x faster!)
```

---

## Cache Directory Structure

```
~/.cache/
├── synapse/
│   ├── schemas/
│   │   ├── manifest.json         # Global substrate hash
│   │   ├── cone.json
│   │   ├── arbor.json
│   │   └── ...
│   └── ir/
│       ├── manifest.json         # IR version, dependencies
│       ├── cone.ir.json
│       ├── arbor.ir.json
│       └── ...
└── hub-codegen/
    ├── rust/
    │   ├── manifest.json         # Target, IR hash mapping
    │   ├── cone/
    │   │   ├── hash.txt
    │   │   ├── types.rs
    │   │   └── methods.rs
    │   ├── arbor/
    │   └── ...
    └── typescript/
        ├── manifest.json
        ├── cone/
        └── ...
```

---

## Implementation Plan

### Phase 1: Schema Caching (Synapse)
- [ ] Add `--use-cache` flag to synapse
- [ ] Implement `Synapse.Cache.Schema` module
- [ ] Store schemas in `~/.cache/synapse/schemas/`
- [ ] Validate cache using global substrate hash
- [ ] Fetch only changed plugin schemas

### Phase 2: IR Caching (Synapse)
- [ ] Split IR generation per plugin
- [ ] Track plugin dependencies
- [ ] Store IR fragments in `~/.cache/synapse/ir/`
- [ ] Implement smart merge of cached + fresh IR
- [ ] Add transitive dependency invalidation

### Phase 3: Code Caching (hub-codegen)
- [ ] Add `--use-cache` flag to hub-codegen
- [ ] Implement `cache.rs` module
- [ ] Group IR by plugin/namespace
- [ ] Store generated code in `~/.cache/hub-codegen/{target}/`
- [ ] Hash plugin IR fragments for cache keys
- [ ] Copy cached code when IR unchanged

### Phase 4: Integration
- [ ] Update `scripts/update-rust-client.sh` to use caching
- [ ] Add Docker support for persistent caches
- [ ] Add cache cleanup/invalidation commands
- [ ] Document cache behavior

---

## Cache Management

### Manual Operations
```bash
# Clear all caches
rm -rf ~/.cache/synapse ~/.cache/hub-codegen

# Clear schema cache only
rm -rf ~/.cache/synapse/schemas

# Clear IR cache only
rm -rf ~/.cache/synapse/ir

# Clear code cache for specific target
rm -rf ~/.cache/hub-codegen/rust

# Inspect cache
synapse --cache-info
hub-codegen --cache-info
```

### Automatic Cleanup
```bash
# Clean caches older than 7 days
synapse --cache-clean --max-age 7d
hub-codegen --cache-clean --max-age 7d
```

---

## Benefits Summary

| Metric | Without Cache | With Cache | Speedup |
|--------|---------------|------------|---------|
| Schema fetch | 5-10s | 0.5s | 10-20x |
| IR generation | 5-10s | 1-2s | 3-5x |
| Code generation | 10-20s | 1-2s | 5-10x |
| Cargo check | 30-60s | 5-10s | 3-6x |
| **Total** | **50-100s** | **7-14s** | **7x** |

For large systems with 20+ plugins:
- Without cache: 5-10 minutes
- With cache (1 plugin changed): 10-20 seconds
- **Speedup: 20-30x**

---

## Open Questions

1. **Cache Location**: User home vs project-local vs global?
2. **Cache Format**: JSON (human-readable) vs MessagePack (compact)?
3. **Distributed Caching**: Share cache across CI/team?
4. **Cache Size Limits**: LRU eviction? Max size?
5. **Cross-Version Compat**: Handle IR version changes?

---

## Future Enhancements

1. **Parallel Generation**: Generate multiple plugins in parallel
2. **Watch Mode**: Regenerate on substrate restart automatically
3. **Remote Cache**: Share caches via S3/GCS for CI
4. **Cache Analytics**: Track cache hit/miss rates
5. **Dependency Graph Viz**: Show which plugins depend on which
