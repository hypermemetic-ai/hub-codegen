# hub-codegen Tests

This directory contains tests for the hub-codegen code generator, including comprehensive cache invalidation tests for the V2 granular hash system.

## Test Organization

```
tests/
├── README.md                           # This file
├── CACHE_TEST_HARNESS.md              # Detailed cache test documentation
├── cache_invalidation_test.rs         # Core cache invalidation tests
├── configurable_backend_test.rs       # Configurable mock backend
├── rust_codegen_smoke_test.rs         # Rust code generation smoke tests
└── test_scenarios/                     # JSON test configurations
    ├── README.md                       # Scenario documentation
    ├── scenario_a_initial.json         # Scenario A: Initial config
    ├── scenario_a_modified.json        # Scenario A: Modified config
    ├── scenario_b_initial.json         # Scenario B: Initial config
    ├── scenario_b_modified.json        # Scenario B: Modified config
    ├── scenario_c_initial.json         # Scenario C: Initial config
    └── scenario_c_modified.json        # Scenario C: Modified config
```

## Quick Start

### Run All Tests

```bash
# Run all tests including cache tests
cargo test

# Run only cache tests
./scripts/test-cache.sh

# Run with verbose output
./scripts/test-cache.sh --verbose
```

### Run Specific Test Suites

```bash
# Cache invalidation tests
cargo test --test cache_invalidation_test

# Configurable backend tests
cargo test --test configurable_backend_test

# Rust codegen smoke tests
cargo test --test rust_codegen_smoke_test
```

### Run Specific Scenarios

```bash
# Scenario A: Method-only change
./scripts/test-cache.sh --scenario a

# Scenario B: Children-only change
./scripts/test-cache.sh --scenario b

# Scenario C: Both change
./scripts/test-cache.sh --scenario c
```

## Test Suites

### 1. Cache Invalidation Tests (`cache_invalidation_test.rs`)

Tests the core cache invalidation logic with V2 granular hashes.

**Tests:**
- `test_scenario_a_method_only_change` - Method changes only affect self_hash
- `test_scenario_b_children_only_change` - Children changes only affect children_hash
- `test_scenario_c_both_change` - Both changes affect both hashes
- `test_cache_manifest_operations` - Cache manifest read/write
- `test_cache_invalidation_logic` - Cache hit/miss detection
- `test_granular_hash_computation` - V2 hash computation
- `test_end_to_end_cache_workflow` - Complete cache lifecycle
- `test_hash_computation_performance` - Performance benchmarks

**Run:**
```bash
cargo test --test cache_invalidation_test -- --nocapture
```

### 2. Configurable Backend Tests (`configurable_backend_test.rs`)

Tests the mock backend that generates IR from JSON configuration.

**Tests:**
- `test_backend_config_serialization` - Config file handling
- `test_configurable_backend_ir_generation` - IR generation from config
- `test_scenario_a_method_only_change` - Method-only scenario with backend
- `test_scenario_b_children_only_change` - Children-only scenario with backend
- `test_scenario_c_both_change` - Combined scenario with backend
- `test_multiple_plugins` - Multiple plugin handling
- `test_empty_plugin` - Edge case testing

**Run:**
```bash
cargo test --test configurable_backend_test -- --nocapture
```

### 3. Rust Codegen Smoke Tests (`rust_codegen_smoke_test.rs`)

End-to-end tests that generate Rust code and verify it compiles.

**Tests:**
- `test_generated_rust_compiles` - Full compilation test
- `test_generated_code_structure` - Code structure validation
- `test_no_warnings` - Warning-free generation

**Run:**
```bash
cargo test --test rust_codegen_smoke_test -- --nocapture
```

## Test Scenarios

### Scenario A: Method-Only Change

**Purpose:** Validate that method changes only invalidate `self_hash`, not `children_hash`.

**Cache Impact:** Regenerate methods only, reuse cached children → ~50% faster.

**Files:**
- `test_scenarios/scenario_a_initial.json`
- `test_scenarios/scenario_a_modified.json`

### Scenario B: Children-Only Change

**Purpose:** Validate that children changes only invalidate `children_hash`, not `self_hash`.

**Cache Impact:** Reuse cached methods, regenerate children → ~50% faster.

**Files:**
- `test_scenarios/scenario_b_initial.json`
- `test_scenarios/scenario_b_modified.json`

### Scenario C: Both Change

**Purpose:** Validate that changing both methods and children invalidates both hashes.

**Cache Impact:** Full regeneration required, no cache benefits.

**Files:**
- `test_scenarios/scenario_c_initial.json`
- `test_scenarios/scenario_c_modified.json`

## Example Programs

Located in `examples/`:

### 1. Generate from Config

Generate IR from a JSON configuration file:

```bash
cargo run --example generate_from_config tests/test_scenarios/scenario_a_initial.json
```

Output:
```
=== Configuration ===
Plugin: test_plugin
  Methods: 3
    - method1
    - method2
    - method3
  Children: 2
    - child1
    - child2

=== Hash Computation ===
Plugin: test_plugin
  self_hash (methods):   abc123...
  children_hash:         def456...
  composite_hash:        789xyz...
```

### 2. Compare Configs

Compare two configuration files and show hash differences:

```bash
cargo run --example compare_configs \
  tests/test_scenarios/scenario_a_initial.json \
  tests/test_scenarios/scenario_a_modified.json
```

Output:
```
=== Plugin: test_plugin ===

Methods:
  Config 1: 3 methods, hash: abc123...
  Config 2: 2 methods, hash: xyz789...
  ❌ CHANGED - self_hash will be invalidated
    Removed Methods:
      - method3

Children:
  Config 1: 2 children, hash: def456...
  Config 2: 2 children, hash: def456...
  ✅ UNCHANGED

=== Cache Invalidation Impact ===
  ⚠️  self_hash will change → Regenerate method bindings
  ✅ children_hash unchanged → Reuse cached child bindings
```

## Performance Targets

From `CACHE_CONTRACTS.md`:

| Operation | Target | Actual | Status |
|-----------|--------|--------|--------|
| Cache manifest read | < 10ms | ~1ms | ✅ |
| Single plugin read | < 50ms | ~5ms | ✅ |
| Hash computation | < 50ms | ~96µs | ✅ |
| 1000 hashes | N/A | ~96ms | ✅ |

## Writing New Tests

### 1. Using TestIRBuilder

```rust
use hub_codegen::ir::*;

let ir = TestIRBuilder::new("my_plugin")
    .with_method("my_method")
    .with_type("MyType")
    .build();

// Test the IR...
```

### 2. Using ConfigurableBackend

```rust
let mut config = BackendConfig {
    plugins: HashMap::new(),
};

config.plugins.insert(
    "my_plugin".to_string(),
    PluginConfig {
        methods: vec!["method1".to_string()],
        children: vec![],
        types: vec!["Type1".to_string()],
    },
);

let backend = ConfigurableBackend::new(config);
let ir = backend.generate_ir();

// Test the IR...
```

### 3. Creating Test Scenarios

1. Create JSON config files in `test_scenarios/`
2. Add test case in appropriate test file
3. Document expected behavior in `CACHE_TEST_HARNESS.md`
4. Run tests to validate

## CI/CD Integration

### GitHub Actions

```yaml
- name: Run Cache Tests
  run: |
    cargo test --test cache_invalidation_test
    cargo test --test configurable_backend_test
    ./scripts/test-cache.sh
```

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

# Run cache tests before commit
./scripts/test-cache.sh || exit 1
```

## Debugging

### Test Failures

```bash
# Run with backtrace
RUST_BACKTRACE=1 cargo test test_name

# Run with detailed output
cargo test test_name -- --nocapture --test-threads=1

# Run specific scenario
./scripts/test-cache.sh --verbose --scenario a
```

### Cache Issues

```bash
# Check cache directory
ls -la ~/.cache/plexus-codegen/hub-codegen/

# Clear cache
rm -rf ~/.cache/plexus-codegen/

# Verify config files
jq . tests/test_scenarios/scenario_a_initial.json
```

### Hash Mismatches

```bash
# Compare configs manually
cargo run --example compare_configs config1.json config2.json

# Generate IR and inspect hashes
cargo run --example generate_from_config config.json
```

## References

- **CACHE_TEST_HARNESS.md**: Detailed cache test documentation
- **../CACHE_CONTRACTS.md**: Cache system contracts
- **../INCREMENTAL_CODEGEN.md**: Overall architecture
- **../src/cache.rs**: Cache implementation
- **../src/hash.rs**: Hash utilities

## Contributing

When adding tests:

1. Follow existing patterns (`TestIRBuilder`, `ConfigurableBackend`)
2. Add documentation to `CACHE_TEST_HARNESS.md`
3. Update this README if adding new test suites
4. Ensure tests run in < 1 second (for fast CI)
5. Validate performance targets
6. Support both V1 and V2 cache formats

## Test Coverage

Current coverage (as of latest):

- ✅ Core cache operations
- ✅ V2 granular hashing
- ✅ Cache invalidation logic
- ✅ All three test scenarios
- ✅ Performance benchmarks
- ✅ Edge cases
- ✅ End-to-end workflows

## Known Limitations

- Tests use temporary directories (cleaned up automatically)
- Some tests modify environment variables (isolated per test)
- Performance tests may be affected by system load
- Tests assume Linux/macOS file system behavior

## Future Work

- [ ] HTTP test backend for integration testing
- [ ] Dependency graph validation tests
- [ ] Concurrent access testing
- [ ] Cache size limit testing
- [ ] Remote cache backend testing
- [ ] Watch mode testing

## Support

For issues or questions:

1. Check `CACHE_TEST_HARNESS.md` for detailed documentation
2. Review test output with `--nocapture`
3. Use example programs to debug config issues
4. Check `CACHE_CONTRACTS.md` for expected behavior
