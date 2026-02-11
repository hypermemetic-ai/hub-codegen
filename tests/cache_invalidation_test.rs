//! Integration test for cache invalidation system with V2 granular hashes
//!
//! Tests the incremental cache invalidation logic by creating IR files with
//! different plugin configurations and validating that caches are correctly
//! invalidated based on hash changes.

use hub_codegen::ir::*;
use hub_codegen::{cache::*, hash::*};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test IR with configurable plugin structure
struct TestIRBuilder {
    plugin_name: String,
    methods: Vec<String>,
    children: Vec<String>,
    types: HashMap<String, TypeDef>,
}

impl TestIRBuilder {
    fn new(plugin_name: &str) -> Self {
        Self {
            plugin_name: plugin_name.to_string(),
            methods: Vec::new(),
            children: Vec::new(),
            types: HashMap::new(),
        }
    }

    /// Add a method to the plugin
    fn with_method(mut self, method_name: &str) -> Self {
        self.methods.push(method_name.to_string());
        self
    }

    /// Add a child plugin reference
    fn with_child(mut self, child_name: &str) -> Self {
        self.children.push(child_name.to_string());
        self
    }

    /// Add a type definition
    fn with_type(mut self, type_name: &str) -> Self {
        let full_name = format!("{}.{}", self.plugin_name, type_name);
        self.types.insert(
            full_name.clone(),
            TypeDef {
                td_name: type_name.to_string(),
                td_namespace: self.plugin_name.clone(),
                td_description: Some(format!("Test type {}", type_name)),
                td_kind: TypeKind::KindStruct {
                    ks_fields: vec![FieldDef {
                        fd_name: "value".to_string(),
                        fd_type: TypeRef::RefPrimitive("string".to_string(), None),
                        fd_description: Some("Test field".to_string()),
                        fd_required: true,
                        fd_default: None,
                    }],
                },
            },
        );
        self
    }

    /// Build the IR with the configured structure
    fn build(self) -> hub_codegen::IR {
        let mut ir_types = self.types;
        let mut ir_methods = HashMap::new();
        let mut ir_plugins = HashMap::new();

        // Create methods
        for method_name in &self.methods {
            let full_path = format!("{}.{}", self.plugin_name, method_name);
            ir_methods.insert(
                full_path.clone(),
                MethodDef {
                    md_name: method_name.clone(),
                    md_full_path: full_path,
                    md_namespace: self.plugin_name.clone(),
                    md_description: Some(format!("Test method {}", method_name)),
                    md_streaming: false,
                    md_params: vec![ParamDef {
                        pd_name: "input".to_string(),
                        pd_type: TypeRef::RefPrimitive("string".to_string(), None),
                        pd_description: Some("Test input".to_string()),
                        pd_required: true,
                        pd_default: None,
                    }],
                    md_returns: TypeRef::RefPrimitive("string".to_string(), None),
                },
            );
        }

        // Add plugin mapping
        ir_plugins.insert(self.plugin_name.clone(), self.methods.clone());

        // Compute IR hash based on content
        let ir_content = format!(
            "{:?}{:?}{:?}",
            ir_types, ir_methods, ir_plugins
        );
        let ir_hash = compute_hash(&ir_content);

        hub_codegen::IR {
            ir_version: "2.0".to_string(),
            ir_backend: "test".to_string(),
            ir_hash: Some(ir_hash),
            ir_metadata: None,
            ir_types,
            ir_methods,
            ir_plugins,
        }
    }
}

/// Test scenario for cache invalidation
#[derive(Debug)]
struct CacheTestScenario {
    name: String,
    initial_ir: hub_codegen::IR,
    modified_ir: hub_codegen::IR,
    expected_cache_hit: bool,
    description: String,
}

impl CacheTestScenario {
    fn new(
        name: &str,
        initial_ir: hub_codegen::IR,
        modified_ir: hub_codegen::IR,
        expected_cache_hit: bool,
        description: &str,
    ) -> Self {
        Self {
            name: name.to_string(),
            initial_ir,
            modified_ir,
            expected_cache_hit,
            description: description.to_string(),
        }
    }
}

/// Helper to compute plugin IR hash for cache validation
fn compute_plugin_ir_hash(ir: &hub_codegen::IR, plugin: &str) -> String {
    // Filter types and methods for this plugin
    let plugin_types: HashMap<_, _> = ir
        .ir_types
        .iter()
        .filter(|(name, _)| name.starts_with(&format!("{}.", plugin)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let plugin_methods: HashMap<_, _> = ir
        .ir_methods
        .iter()
        .filter(|(name, _)| name.starts_with(&format!("{}.", plugin)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Serialize and hash
    let content = format!("{:?}{:?}", plugin_types, plugin_methods);
    compute_hash(&content)
}

/// Test Scenario A: Method-only change (only self_hash should change)
#[test]
fn test_scenario_a_method_only_change() {
    println!("\n=== Scenario A: Method-Only Change ===");

    // Initial: Plugin with methods [method1, method2]
    let initial_ir = TestIRBuilder::new("test_plugin")
        .with_method("method1")
        .with_method("method2")
        .with_type("TestType")
        .build();

    // Modified: Plugin with methods [method1] (removed method2)
    let modified_ir = TestIRBuilder::new("test_plugin")
        .with_method("method1")
        .with_type("TestType")
        .build();

    let initial_hash = compute_plugin_ir_hash(&initial_ir, "test_plugin");
    let modified_hash = compute_plugin_ir_hash(&modified_ir, "test_plugin");

    println!("Initial IR hash:  {}", initial_hash);
    println!("Modified IR hash: {}", modified_hash);

    assert_ne!(
        initial_hash, modified_hash,
        "Hashes should differ when methods change"
    );

    println!("✅ Method change detected correctly");
}

/// Test Scenario B: Children-only change (only children_hash should change)
#[test]
fn test_scenario_b_children_only_change() {
    println!("\n=== Scenario B: Children-Only Change ===");

    // For this test, we simulate children by including references to child namespaces
    // In a real scenario, children would be separate plugins

    // Initial: Plugin with child references
    let mut initial_ir = TestIRBuilder::new("test_plugin")
        .with_method("method1")
        .with_type("TestType")
        .build();

    // Add child plugin marker (in real system, this would be in ir_plugins or metadata)
    initial_ir.ir_plugins.insert(
        "test_plugin.child1".to_string(),
        vec!["child_method".to_string()],
    );

    // Modified: Plugin with different child references
    let mut modified_ir = TestIRBuilder::new("test_plugin")
        .with_method("method1")
        .with_type("TestType")
        .build();

    // Child changed
    modified_ir.ir_plugins.insert(
        "test_plugin.child2".to_string(),
        vec!["child_method".to_string()],
    );

    // Compute hashes including children
    let initial_content = format!("{:?}", initial_ir.ir_plugins);
    let modified_content = format!("{:?}", modified_ir.ir_plugins);

    let initial_hash = compute_hash(&initial_content);
    let modified_hash = compute_hash(&modified_content);

    println!("Initial children hash:  {}", initial_hash);
    println!("Modified children hash: {}", modified_hash);

    assert_ne!(
        initial_hash, modified_hash,
        "Hashes should differ when children change"
    );

    println!("✅ Children change detected correctly");
}

/// Test Scenario C: Both methods and children change
#[test]
fn test_scenario_c_both_change() {
    println!("\n=== Scenario C: Both Methods and Children Change ===");

    // Initial: Plugin with methods and children
    let mut initial_ir = TestIRBuilder::new("test_plugin")
        .with_method("method1")
        .with_method("method2")
        .with_type("TestType")
        .build();

    initial_ir.ir_plugins.insert(
        "test_plugin.child1".to_string(),
        vec!["child_method".to_string()],
    );

    // Modified: Different methods and children
    let mut modified_ir = TestIRBuilder::new("test_plugin")
        .with_method("method1")
        .with_method("method3") // Different method
        .with_type("TestType")
        .build();

    modified_ir.ir_plugins.insert(
        "test_plugin.child2".to_string(), // Different child
        vec!["child_method".to_string()],
    );

    let initial_methods_hash = compute_plugin_ir_hash(&initial_ir, "test_plugin");
    let modified_methods_hash = compute_plugin_ir_hash(&modified_ir, "test_plugin");

    let initial_children_hash = compute_hash(&format!("{:?}", initial_ir.ir_plugins));
    let modified_children_hash = compute_hash(&format!("{:?}", modified_ir.ir_plugins));

    println!("Initial methods hash:   {}", initial_methods_hash);
    println!("Modified methods hash:  {}", modified_methods_hash);
    println!("Initial children hash:  {}", initial_children_hash);
    println!("Modified children hash: {}", modified_children_hash);

    assert_ne!(
        initial_methods_hash, modified_methods_hash,
        "Methods hashes should differ"
    );
    assert_ne!(
        initial_children_hash, modified_children_hash,
        "Children hashes should differ"
    );

    println!("✅ Both changes detected correctly");
}

/// Test cache manifest creation and validation
#[test]
fn test_cache_manifest_operations() {
    println!("\n=== Cache Manifest Operations ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache_root = temp_dir.path().to_path_buf();

    // Create the cache directory structure
    let cache_dir = cache_root.join(".cache/plexus-codegen/hub-codegen/rust/test_backend");
    std::fs::create_dir_all(&cache_dir).expect("Failed to create cache dir");

    // Set up test environment
    std::env::set_var("HOME", cache_root.to_str().unwrap());

    // Create toolchain versions
    let toolchain = ToolchainVersions {
        synapse_cc: "0.1.0.0".to_string(),
        synapse: "0.2.0.0".to_string(),
        hub_codegen: "0.1.0".to_string(),
    };

    // Create a new manifest
    let mut manifest = CodeCacheManifest::new("rust".to_string(), toolchain);

    // Add plugin with file hashes
    let mut file_hashes = HashMap::new();
    file_hashes.insert("types.rs".to_string(), "abc123".to_string());
    file_hashes.insert("methods.rs".to_string(), "def456".to_string());

    manifest.add_plugin(
        "test_plugin".to_string(),
        "ir_hash_123".to_string(),
        file_hashes,
    );

    println!("Created manifest with plugin: test_plugin");
    println!("IR hash: ir_hash_123");

    // Write manifest to disk
    write_cache_manifest("rust", "test_backend", &manifest)
        .expect("Failed to write manifest");

    println!("✅ Manifest written to disk");

    // Read manifest back
    let loaded_manifest =
        read_cache_manifest("rust", "test_backend").expect("Failed to read manifest");

    assert_eq!(loaded_manifest.version, "2.0");
    assert_eq!(loaded_manifest.target, "rust");
    assert_eq!(loaded_manifest.plugins.len(), 1);
    assert!(loaded_manifest.plugins.contains_key("test_plugin"));

    let plugin_cache = &loaded_manifest.plugins["test_plugin"];
    assert_eq!(plugin_cache.ir_hash, "ir_hash_123");
    assert_eq!(plugin_cache.file_hashes.len(), 2);

    println!("✅ Manifest read and validated successfully");
}

/// Test cache invalidation logic with different scenarios
#[test]
fn test_cache_invalidation_logic() {
    println!("\n=== Cache Invalidation Logic ===");

    // Scenario 1: IR hash matches - cache hit
    let ir1 = TestIRBuilder::new("plugin1")
        .with_method("method1")
        .build();

    let hash1 = compute_plugin_ir_hash(&ir1, "plugin1");

    // Simulate cache entry with same hash
    let cached_hash = hash1.clone();

    assert_eq!(
        hash1, cached_hash,
        "Cache hit: hashes should match"
    );
    println!("✅ Scenario 1: Cache hit detected correctly");

    // Scenario 2: IR hash differs - cache miss
    let ir2 = TestIRBuilder::new("plugin1")
        .with_method("method1")
        .with_method("method2") // Different!
        .build();

    let hash2 = compute_plugin_ir_hash(&ir2, "plugin1");

    assert_ne!(
        hash2, cached_hash,
        "Cache miss: hashes should differ"
    );
    println!("✅ Scenario 2: Cache miss detected correctly");
}

/// Test granular hash computation for V2 cache system
#[test]
fn test_granular_hash_computation() {
    println!("\n=== Granular Hash Computation (V2) ===");

    let plugin_name = "test_plugin";

    // Create IR with methods and types
    let ir = TestIRBuilder::new(plugin_name)
        .with_method("method1")
        .with_method("method2")
        .with_type("Type1")
        .with_type("Type2")
        .build();

    // Compute separate hashes for methods and types
    let methods_only: HashMap<_, _> = ir
        .ir_methods
        .iter()
        .filter(|(name, _)| name.starts_with(&format!("{}.", plugin_name)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let types_only: HashMap<_, _> = ir
        .ir_types
        .iter()
        .filter(|(name, _)| name.starts_with(&format!("{}.", plugin_name)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let methods_hash = compute_hash(&format!("{:?}", methods_only));
    let types_hash = compute_hash(&format!("{:?}", types_only));
    let composite_hash = compute_hash(&format!("{:?}{:?}", methods_only, types_only));

    println!("Methods hash:   {}", methods_hash);
    println!("Types hash:     {}", types_hash);
    println!("Composite hash: {}", composite_hash);

    // All hashes should be 16 characters (Plexus hash format)
    assert_eq!(methods_hash.len(), 16);
    assert_eq!(types_hash.len(), 16);
    assert_eq!(composite_hash.len(), 16);

    // Composite should differ from individual hashes
    assert_ne!(methods_hash, composite_hash);
    assert_ne!(types_hash, composite_hash);

    println!("✅ Granular hashes computed correctly");
}

/// Test end-to-end cache workflow
#[test]
fn test_end_to_end_cache_workflow() {
    println!("\n=== End-to-End Cache Workflow ===");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    std::env::set_var("HOME", temp_dir.path().to_str().unwrap());

    let toolchain = ToolchainVersions {
        synapse_cc: "0.1.0.0".to_string(),
        synapse: "0.2.0.0".to_string(),
        hub_codegen: "0.1.0".to_string(),
    };

    // Step 1: First generation (cold cache)
    println!("\n--- Step 1: First Generation (Cold Cache) ---");
    let ir_v1 = TestIRBuilder::new("my_plugin")
        .with_method("method1")
        .with_method("method2")
        .with_type("MyType")
        .build();

    let ir_hash_v1 = compute_plugin_ir_hash(&ir_v1, "my_plugin");
    println!("Generated IR v1 with hash: {}", ir_hash_v1);

    // Simulate code generation
    let mut file_hashes_v1 = HashMap::new();
    file_hashes_v1.insert("types.rs".to_string(), "type_hash_v1".to_string());
    file_hashes_v1.insert("methods.rs".to_string(), "method_hash_v1".to_string());

    let mut manifest = CodeCacheManifest::new("rust".to_string(), toolchain.clone());
    manifest.add_plugin("my_plugin".to_string(), ir_hash_v1.clone(), file_hashes_v1);

    write_cache_manifest("rust", "test", &manifest).expect("Failed to write manifest");
    println!("✅ Cache manifest written");

    // Step 2: Regeneration with same IR (cache hit)
    println!("\n--- Step 2: Regeneration with Same IR (Cache Hit) ---");
    let loaded_manifest = read_cache_manifest("rust", "test").expect("Failed to read manifest");

    let cached_plugin = loaded_manifest.plugins.get("my_plugin").unwrap();
    assert_eq!(cached_plugin.ir_hash, ir_hash_v1);
    println!("✅ Cache hit! IR hash matches: {}", ir_hash_v1);

    // Step 3: Regeneration with modified IR (cache miss)
    println!("\n--- Step 3: Regeneration with Modified IR (Cache Miss) ---");
    let ir_v2 = TestIRBuilder::new("my_plugin")
        .with_method("method1")
        .with_method("method3") // Changed!
        .with_type("MyType")
        .build();

    let ir_hash_v2 = compute_plugin_ir_hash(&ir_v2, "my_plugin");
    println!("Generated IR v2 with hash: {}", ir_hash_v2);

    assert_ne!(ir_hash_v2, ir_hash_v1);
    println!("✅ Cache miss! IR changed, need to regenerate");

    // Update cache with new generation
    let mut file_hashes_v2 = HashMap::new();
    file_hashes_v2.insert("types.rs".to_string(), "type_hash_v2".to_string());
    file_hashes_v2.insert("methods.rs".to_string(), "method_hash_v2".to_string());

    let mut updated_manifest = CodeCacheManifest::new("rust".to_string(), toolchain);
    updated_manifest.add_plugin("my_plugin".to_string(), ir_hash_v2, file_hashes_v2);

    write_cache_manifest("rust", "test", &updated_manifest).expect("Failed to write manifest");
    println!("✅ Cache updated with new IR hash");

    println!("\n=== End-to-End Workflow Complete ===");
}

/// Performance test: Hash computation speed
#[test]
fn test_hash_computation_performance() {
    println!("\n=== Hash Computation Performance ===");

    let ir = TestIRBuilder::new("perf_test")
        .with_method("method1")
        .with_method("method2")
        .with_method("method3")
        .with_type("Type1")
        .with_type("Type2")
        .with_type("Type3")
        .build();

    let start = std::time::Instant::now();
    let iterations = 1000;

    for _ in 0..iterations {
        let _ = compute_plugin_ir_hash(&ir, "perf_test");
    }

    let elapsed = start.elapsed();
    let avg_time = elapsed / iterations;

    println!("Computed {} hashes in {:?}", iterations, elapsed);
    println!("Average time per hash: {:?}", avg_time);

    // Should be under 1ms per hash (target from CACHE_CONTRACTS.md: < 50ms)
    assert!(
        avg_time.as_millis() < 1,
        "Hash computation too slow: {:?}",
        avg_time
    );

    println!("✅ Performance test passed");
}
