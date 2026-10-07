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

## Implementation Waves (Parallel Execution)

This section breaks down the implementation into waves, where tasks within each wave can be executed in parallel by different agents/developers.

### Wave 1: Foundation (All tasks can run in parallel)

**Agent A: Synapse Schema Cache Module**
- [ ] Create `Synapse/Cache/Schema.hs` module
- [ ] Implement `SchemaCache` data type
- [ ] Implement cache loading/saving (JSON format)
- [ ] Add hash validation logic
- [ ] Write unit tests for cache operations

**Agent B: Synapse IR Cache Module**
- [ ] Create `Synapse/Cache/IR.hs` module
- [ ] Implement `IRCache` data type with per-plugin fragments
- [ ] Implement dependency tracking data structures
- [ ] Add IR fragment merging logic
- [ ] Write unit tests for IR merging

**Agent C: hub-codegen Cache Module**
- [ ] Create `src/cache.rs` module in hub-codegen
- [ ] Implement `CodeCache` struct
- [ ] Add hash computation for IR fragments
- [ ] Implement cache directory management
- [ ] Write unit tests for cache operations

**Agent D: Shared Cache Infrastructure**
- [ ] Design common cache directory structure
- [ ] Document how to use Plexus's built-in hashes (PluginSchema.hash, plexus_hash)
- [ ] Create cache manifest format specification
- [ ] Add cache cleanup utilities
- [ ] Document cache file formats

**Note:** Plexus already provides content-based hashes at all levels. No need to
implement custom hashing - just read `schema.hash` and `plexus_hash` fields!

---

### Wave 2: Integration (Dependencies on Wave 1)

**Agent A: Synapse CLI Integration**
- [ ] Add `--use-cache` flag to Main.hs
- [ ] Add `--cache-dir` flag to Main.hs
- [ ] Integrate SchemaCache into schema fetching
- [ ] Integrate IRCache into IR building
- [ ] Add `--cache-info` command
- [ ] Add `--cache-clean` command

**Agent B: hub-codegen CLI Integration**
- [ ] Add `--use-cache` flag to main.rs
- [ ] Add `--cache-dir` flag to main.rs
- [ ] Integrate CodeCache into generation pipeline
- [ ] Add per-plugin IR grouping logic
- [ ] Add `--cache-info` command
- [ ] Add `--cache-clean` command

**Agent C: Dependency Graph Analysis**
- [ ] Implement plugin dependency extraction from IR
- [ ] Build dependency graph data structure
- [ ] Implement transitive dependency calculation
- [ ] Add cache invalidation based on dependencies
- [ ] Write tests for dependency scenarios

**Agent D: Script Updates**
- [ ] Update `scripts/update-rust-client.sh` to use caching
- [ ] Update `scripts/update-client.sh` to use caching
- [ ] Add cache directory parameter to scripts
- [ ] Add cache stats reporting to scripts
- [ ] Update documentation in `scripts/README.md`

---

### Wave 3: Testing & Optimization (Dependencies on Wave 2)

**Agent A: Integration Testing**
- [ ] Create end-to-end test for schema caching
- [ ] Create end-to-end test for IR caching
- [ ] Create end-to-end test for code caching
- [ ] Test cache invalidation scenarios
- [ ] Test dependency-based invalidation

**Agent B: Performance Testing**
- [ ] Benchmark cache hit vs miss performance
- [ ] Measure speedup for various change scenarios
- [ ] Profile cache lookup overhead
- [ ] Optimize cache serialization format if needed
- [ ] Document performance characteristics

**Agent C: Docker Integration**
- [ ] Add cache volume mounts to Dockerfile
- [ ] Update docker-compose.yml with cache volumes
- [ ] Test caching in Docker environment
- [ ] Document Docker cache usage
- [ ] Add cache persistence examples

**Agent D: Documentation & Polish**
- [ ] Write user guide for cache usage
- [ ] Add troubleshooting section
- [ ] Create cache management best practices
- [ ] Add examples to README files
- [ ] Create migration guide from non-cached builds

---

### Wave 4: Advanced Features (Optional, can run in parallel)

**Agent A: Watch Mode**
- [ ] Implement file watching for substrate changes
- [ ] Auto-regenerate on schema changes
- [ ] Add debouncing for rapid changes
- [ ] Integrate with cache system
- [ ] Add CLI flags for watch mode

**Agent B: Parallel Generation**
- [ ] Identify independent plugins that can be parallelized
- [ ] Implement parallel IR generation
- [ ] Implement parallel code generation
- [ ] Add worker pool management
- [ ] Benchmark parallel vs sequential

**Agent C: Remote Cache**
- [ ] Design remote cache protocol
- [ ] Implement S3/GCS backend
- [ ] Add cache upload/download commands
- [ ] Implement cache sharing for CI
- [ ] Document remote cache setup

**Agent D: Analytics & Visualization**
- [ ] Add cache hit/miss rate tracking
- [ ] Implement dependency graph visualization
- [ ] Create cache size monitoring
- [ ] Add performance metrics collection
- [ ] Build simple dashboard/CLI display

---

## Task Dependencies Graph

```
Wave 1 (Parallel)
├─ Agent A: Schema Cache Module
├─ Agent B: IR Cache Module
├─ Agent C: Code Cache Module
└─ Agent D: Shared Infrastructure
       │
       ▼
Wave 2 (Depends on Wave 1)
├─ Agent A: Synapse CLI (depends on Schema + IR Cache)
├─ Agent B: hub-codegen CLI (depends on Code Cache)
├─ Agent C: Dependency Graph (depends on IR Cache)
└─ Agent D: Script Updates (depends on all CLIs)
       │
       ▼
Wave 3 (Depends on Wave 2)
├─ Agent A: Integration Tests (depends on all Wave 2)
├─ Agent B: Performance Tests (depends on all Wave 2)
├─ Agent C: Docker Integration (depends on all Wave 2)
└─ Agent D: Documentation (depends on all Wave 2)
       │
       ▼
Wave 4 (Optional, Parallel)
├─ Agent A: Watch Mode
├─ Agent B: Parallel Generation
├─ Agent C: Remote Cache
└─ Agent D: Analytics
```

---

## Estimated Timeline

| Wave | Duration (with 4 parallel agents) | Dependencies |
|------|-----------------------------------|--------------|
| Wave 1 | 2-3 days | None |
| Wave 2 | 3-4 days | Wave 1 complete |
| Wave 3 | 2-3 days | Wave 2 complete |
| Wave 4 | 3-5 days (optional) | Wave 3 complete |
| **Total** | **7-10 days** (or 10-15 days with Wave 4) | Sequential |

With serial execution (single developer): 4-6 weeks

**Speedup from parallelization: 4-6x**

---

## Quick Start for Each Agent

### Agent A (Wave 1)
```bash
cd /workspace/hypermemetic/synapse
git checkout -b feature/schema-cache
mkdir -p src/Synapse/Cache
touch src/Synapse/Cache/Schema.hs
# Start implementing SchemaCache...
```

### Agent B (Wave 1)
```bash
cd /workspace/hypermemetic/synapse
git checkout -b feature/ir-cache
touch src/Synapse/Cache/IR.hs
# Start implementing IRCache...
```

### Agent C (Wave 1)
```bash
cd /workspace/hypermemetic/hub-codegen
git checkout -b feature/code-cache
mkdir -p src/cache
touch src/cache/mod.rs
touch src/cache/code_cache.rs
# Start implementing CodeCache...
```

### Agent D (Wave 1)
```bash
cd /workspace/hypermemetic/hub-codegen
git checkout -b feature/cache-infrastructure
mkdir -p docs/cache
touch docs/cache/CACHE_FORMAT.md
# Document cache formats and shared utilities...
```

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
