# Cache Invalidation Test Scenarios

This directory contains JSON configuration files for testing incremental cache invalidation with the configurable backend.

## Test Scenarios

### Scenario A: Method-Only Change

Tests that removing/adding methods only changes `self_hash`, not `children_hash`.

**Files:**
- `scenario_a_initial.json` - Initial configuration with all methods
- `scenario_a_modified.json` - Configuration with one method removed

**Expected behavior:**
- `self_hash` should change
- `children_hash` should NOT change

### Scenario B: Children-Only Change

Tests that removing/adding children only changes `children_hash`, not `self_hash`.

**Files:**
- `scenario_b_initial.json` - Initial configuration with all children
- `scenario_b_modified.json` - Configuration with one child removed

**Expected behavior:**
- `self_hash` should NOT change
- `children_hash` should change

### Scenario C: Both Change

Tests that changing both methods and children changes both hashes.

**Files:**
- `scenario_c_initial.json` - Initial configuration
- `scenario_c_modified.json` - Configuration with methods and children changed

**Expected behavior:**
- `self_hash` should change
- `children_hash` should change

## Usage

### Running Tests

```bash
# Run all cache invalidation tests
cargo test --test cache_invalidation_test

# Run configurable backend tests
cargo test --test configurable_backend_test

# Run specific scenario
cargo test test_scenario_a_method_only_change

# Run with output
cargo test --test cache_invalidation_test -- --nocapture
```

### Manual Testing with Config Files

```bash
# Generate IR from a config file
cargo run --example generate_from_config tests/test_scenarios/scenario_a_initial.json

# Compare two configs
cargo run --example compare_configs \
  tests/test_scenarios/scenario_a_initial.json \
  tests/test_scenarios/scenario_a_modified.json
```

## Configuration Format

```json
{
  "plugins": {
    "plugin_name": {
      "methods": ["method1", "method2"],
      "children": ["child1", "child2"],
      "types": ["Type1", "Type2"]
    }
  }
}
```

## Adding New Test Scenarios

1. Create a new JSON config file in this directory
2. Add corresponding test case in `configurable_backend_test.rs`
3. Document expected behavior above
4. Run tests to validate

## Expected Hash Behavior

| Change Type | `self_hash` | `children_hash` | `composite_hash` |
|-------------|-------------|-----------------|------------------|
| Method add/remove | ✅ Changes | ❌ Unchanged | ✅ Changes |
| Child add/remove | ❌ Unchanged | ✅ Changes | ✅ Changes |
| Type add/remove | ✅ Changes | ❌ Unchanged | ✅ Changes |
| Both methods & children | ✅ Changes | ✅ Changes | ✅ Changes |
| No changes | ❌ Unchanged | ❌ Unchanged | ❌ Unchanged |

## Performance Targets

From `CACHE_CONTRACTS.md`:

- Cache manifest read: < 10ms
- Single plugin read: < 50ms
- Hash computation: < 50ms per plugin

All tests should validate these targets.
