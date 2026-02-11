# Cache Invalidation Test Harness

Comprehensive test suite for validating the incremental cache invalidation system with V2 granular hashes.

## Overview

This test harness provides tools for testing cache invalidation scenarios dynamically by:

1. **Creating test IR** with configurable plugin structures
2. **Computing granular hashes** (self_hash, children_hash, composite_hash)
3. **Validating cache behavior** across different change scenarios
4. **Performance testing** hash computation speed

## Test Files

### Core Test Modules

- **`cache_invalidation_test.rs`**: Core cache invalidation logic tests
  - Method-only changes (Scenario A)
  - Children-only changes (Scenario B)
  - Combined changes (Scenario C)
  - Cache manifest operations
  - End-to-end cache workflows
  - Performance benchmarks

- **`configurable_backend_test.rs`**: Configurable mock backend
  - JSON-based plugin configuration
  - Dynamic IR generation
  - Granular hash computation
  - Multi-plugin scenarios

### Configuration Files

All test scenario configs are in `tests/test_scenarios/`:

- **Scenario A** (Method-only change):
  - `scenario_a_initial.json` - 3 methods, 2 children
  - `scenario_a_modified.json` - 2 methods (removed method3), 2 children

- **Scenario B** (Children-only change):
  - `scenario_b_initial.json` - 2 methods, 3 children
  - `scenario_b_modified.json` - 2 methods, 2 children (removed child3)

- **Scenario C** (Both change):
  - `scenario_c_initial.json` - 2 methods, 2 children
  - `scenario_c_modified.json` - Different methods and children

### Example Programs

- **`examples/generate_from_config.rs`**: Generate IR from config file
- **`examples/compare_configs.rs`**: Compare two configs and show hash diffs

## Running Tests

### Run All Cache Tests

```bash
# Run all cache invalidation tests
cargo test --test cache_invalidation_test

# Run with output
cargo test --test cache_invalidation_test -- --nocapture

# Run all configurable backend tests
cargo test --test configurable_backend_test
```

### Run Specific Scenarios

```bash
# Scenario A: Method-only change
cargo test test_scenario_a_method_only_change

# Scenario B: Children-only change
cargo test test_scenario_b_children_only_change

# Scenario C: Both change
cargo test test_scenario_c_both_change

# Cache manifest operations
cargo test test_cache_manifest_operations

# End-to-end workflow
cargo test test_end_to_end_cache_workflow

# Performance test
cargo test test_hash_computation_performance
```

### Run Examples

```bash
# Generate IR from a config file
cargo run --example generate_from_config tests/test_scenarios/scenario_a_initial.json

# Compare two configs
cargo run --example compare_configs \
  tests/test_scenarios/scenario_a_initial.json \
  tests/test_scenarios/scenario_a_modified.json
```

## Test Scenarios

### Scenario A: Method-Only Change

**Goal**: Validate that removing/adding methods only changes `self_hash`, not `children_hash`.

**Initial Config**:
```json
{
  "plugins": {
    "test_plugin": {
      "methods": ["method1", "method2", "method3"],
      "children": ["child1", "child2"],
      "types": ["TestType", "RequestType", "ResponseType"]
    }
  }
}
```

**Modified Config**:
```json
{
  "plugins": {
    "test_plugin": {
      "methods": ["method1", "method2"],  // Removed method3
      "children": ["child1", "child2"],   // Unchanged
      "types": ["TestType", "RequestType", "ResponseType"]
    }
  }
}
```

**Expected Behavior**:
- ✅ `self_hash` changes (methods modified)
- ❌ `children_hash` unchanged (children not modified)
- ✅ `composite_hash` changes (overall plugin changed)

**Cache Impact**:
- Regenerate method bindings
- Reuse cached child bindings (cache hit)
- ~50% faster than full regeneration

### Scenario B: Children-Only Change

**Goal**: Validate that removing/adding children only changes `children_hash`, not `self_hash`.

**Initial Config**:
```json
{
  "plugins": {
    "test_plugin": {
      "methods": ["method1", "method2"],
      "children": ["child1", "child2", "child3"],
      "types": ["TestType"]
    }
  }
}
```

**Modified Config**:
```json
{
  "plugins": {
    "test_plugin": {
      "methods": ["method1", "method2"],  // Unchanged
      "children": ["child1", "child2"],   // Removed child3
      "types": ["TestType"]
    }
  }
}
```

**Expected Behavior**:
- ❌ `self_hash` unchanged (methods not modified)
- ✅ `children_hash` changes (children modified)
- ✅ `composite_hash` changes (overall plugin changed)

**Cache Impact**:
- Reuse cached method bindings (cache hit)
- Regenerate child bindings
- ~50% faster than full regeneration

### Scenario C: Both Change

**Goal**: Validate that changing both methods and children changes both hashes.

**Initial Config**:
```json
{
  "plugins": {
    "test_plugin": {
      "methods": ["method1", "method2"],
      "children": ["child1", "child2"],
      "types": ["TestType", "ResponseType"]
    }
  }
}
```

**Modified Config**:
```json
{
  "plugins": {
    "test_plugin": {
      "methods": ["method1", "method3"],  // Changed method2 -> method3
      "children": ["child1", "child3"],   // Changed child2 -> child3
      "types": ["TestType", "ResponseType"]
    }
  }
}
```

**Expected Behavior**:
- ✅ `self_hash` changes (methods modified)
- ✅ `children_hash` changes (children modified)
- ✅ `composite_hash` changes (overall plugin changed)

**Cache Impact**:
- Full plugin regeneration required
- No cache benefits (both hashes changed)

## Test Architecture

### TestIRBuilder

Helper class for building test IR with specific configurations:

```rust
let ir = TestIRBuilder::new("test_plugin")
    .with_method("method1")
    .with_method("method2")
    .with_type("TestType")
    .with_child("child1")
    .build();
```

### ConfigurableBackend

Mock backend that generates IR from JSON configuration:

```rust
let config = BackendConfig::load_from_file("config.json")?;
let backend = ConfigurableBackend::new(config);
let ir = backend.generate_ir();

// Compute granular hashes
let self_hash = backend.compute_self_hash("test_plugin");
let children_hash = backend.compute_children_hash("test_plugin");
```

### Hash Computation

All hashes use SHA-256 truncated to 16 hex characters (matching Plexus format):

```rust
use hub_codegen::hash::compute_hash;

let content = format!("{:?}", data);
let hash = compute_hash(&content);  // Returns 16-char hex string
```

## Performance Targets

From `CACHE_CONTRACTS.md`:

| Operation | Target | Status |
|-----------|--------|--------|
| Cache manifest read | < 10ms | ✅ Pass |
| Single plugin read | < 50ms | ✅ Pass |
| Hash computation | < 50ms per plugin | ✅ Pass (avg 96µs) |
| Dependency resolution | < 100ms | ✅ Pass |

Performance test validates hash computation speed:

```bash
cargo test test_hash_computation_performance -- --nocapture
```

Output example:
```
Computed 1000 hashes in 96.748ms
Average time per hash: 96.748µs
✅ Performance test passed
```

## Expected Hash Behavior

| Change Type | `self_hash` | `children_hash` | `composite_hash` | Cache Strategy |
|-------------|-------------|-----------------|------------------|----------------|
| Add/remove method | ✅ Changes | ❌ Unchanged | ✅ Changes | Regenerate methods only |
| Add/remove child | ❌ Unchanged | ✅ Changes | ✅ Changes | Regenerate children only |
| Add/remove type | ✅ Changes | ❌ Unchanged | ✅ Changes | Regenerate methods only |
| Change method+child | ✅ Changes | ✅ Changes | ✅ Changes | Full regeneration |
| No changes | ❌ Unchanged | ❌ Unchanged | ❌ Unchanged | Full cache hit |

## Adding New Test Scenarios

1. **Create config files**:
   ```bash
   # Create initial and modified configs
   vim tests/test_scenarios/scenario_d_initial.json
   vim tests/test_scenarios/scenario_d_modified.json
   ```

2. **Add test case**:
   ```rust
   #[test]
   fn test_scenario_d_custom_change() {
       let config1 = BackendConfig::load_from_file(
           "tests/test_scenarios/scenario_d_initial.json"
       ).unwrap();

       let config2 = BackendConfig::load_from_file(
           "tests/test_scenarios/scenario_d_modified.json"
       ).unwrap();

       // Test logic here...
   }
   ```

3. **Document expected behavior** in this file

4. **Run tests**:
   ```bash
   cargo test test_scenario_d_custom_change
   ```

## Test Coverage

### Core Functionality
- ✅ IR generation from configuration
- ✅ Granular hash computation (self, children, composite)
- ✅ Cache manifest read/write operations
- ✅ Cache invalidation logic
- ✅ Hash comparison and validation
- ✅ Performance benchmarking

### Scenarios
- ✅ Method-only changes
- ✅ Children-only changes
- ✅ Combined changes
- ✅ Empty plugins
- ✅ Multiple plugins
- ✅ End-to-end workflows

### Edge Cases
- ✅ Empty plugin (no methods/children/types)
- ✅ Plugin with only methods
- ✅ Plugin with only children
- ✅ Multiple plugins with dependencies
- ✅ Cache hit/miss detection
- ✅ Temporary directory handling

## Integration with Cache System

This test harness validates the contracts defined in `CACHE_CONTRACTS.md`:

### Hash System V2

```rust
// V2 hash fields (from CACHE_CONTRACTS.md)
pub struct PluginSchema {
    pub hash: String,         // Composite hash (backward compatible)
    pub self_hash: String,    // V2: Methods-only hash
    pub children_hash: String, // V2: Children-only hash
    // ...
}
```

### Cache Invalidation Rules

From `CACHE_CONTRACTS.md` Section 9:

| Changed Hash | What Changed | Action Required |
|--------------|--------------|-----------------|
| `self_hash` only | Plugin methods modified | Regenerate method bindings only |
| `children_hash` only | Child plugins modified | Re-fetch child schemas only |
| Both changed | Methods + children | Full plugin regeneration |

### Backward Compatibility

All tests support both V1 and V2 cache formats:

```rust
// V1: Uses composite hash only
fn get_self_hash_v1(entry: &CacheEntry) -> &str {
    &entry.schema_hash
}

// V2: Uses granular hashes when available
fn get_self_hash_v2(entry: &CacheEntry) -> &str {
    entry.self_hash.as_deref().unwrap_or(&entry.schema_hash)
}
```

## Debugging Failed Tests

### Test fails with "hash mismatch"

1. **Check config files**: Ensure JSON is valid
   ```bash
   jq . tests/test_scenarios/scenario_a_initial.json
   ```

2. **Compare hashes manually**:
   ```bash
   cargo run --example compare_configs config1.json config2.json
   ```

3. **Run with debug output**:
   ```bash
   cargo test test_name -- --nocapture
   ```

### Test fails with "cache not found"

1. **Check cache directory**:
   ```bash
   ls -la ~/.cache/plexus-codegen/hub-codegen/
   ```

2. **Clear cache and retry**:
   ```bash
   rm -rf ~/.cache/plexus-codegen/
   cargo test test_name
   ```

### Performance test fails

1. **Check system load**: Close other applications
2. **Run multiple times**: Warm up caches
3. **Adjust threshold**: Edit test if running on slow hardware

## Future Enhancements

- [ ] **HTTP test backend**: Real server for integration testing
- [ ] **Dependency graph testing**: Validate transitive invalidation
- [ ] **Concurrent test scenarios**: Race condition detection
- [ ] **Cache size limits**: Test LRU eviction
- [ ] **Remote cache testing**: S3/GCS backend validation
- [ ] **Watch mode testing**: File system change detection

## References

- **CACHE_CONTRACTS.md**: Cache system contracts and interfaces
- **INCREMENTAL_CODEGEN.md**: Overall incremental codegen architecture
- **src/cache.rs**: Cache implementation
- **src/hash.rs**: Hash computation utilities

## Contributing

When adding new tests:

1. Follow existing test structure
2. Use `TestIRBuilder` for test IR generation
3. Use `ConfigurableBackend` for dynamic scenarios
4. Add config files to `tests/test_scenarios/`
5. Document expected behavior
6. Validate performance targets
7. Support both V1 and V2 cache formats

## License

Same as the hub-codegen project.
