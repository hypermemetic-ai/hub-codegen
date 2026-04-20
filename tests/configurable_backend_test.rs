//! Configurable test backend for testing incremental cache invalidation
//!
//! This module provides a mock backend that can be configured via JSON to
//! expose different plugins, methods, and children. This allows testing
//! cache invalidation scenarios dynamically.

use hub_codegen::ir::*;
use hub_codegen::hash::compute_hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Configuration for the test backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Plugins exposed by the backend
    pub plugins: HashMap<String, PluginConfig>,
}

/// Configuration for a single plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Methods to expose in this plugin
    pub methods: Vec<String>,
    /// Child plugins to expose
    #[serde(default)]
    pub children: Vec<String>,
    /// Types to expose in this plugin
    #[serde(default)]
    pub types: Vec<String>,
}

impl BackendConfig {
    /// Load configuration from JSON file
    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: BackendConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to JSON file
    pub fn save_to_file(&self, path: &Path) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Create a default test configuration
    pub fn default_test_config() -> Self {
        let mut plugins = HashMap::new();

        plugins.insert(
            "test_plugin".to_string(),
            PluginConfig {
                methods: vec!["method1".to_string(), "method2".to_string()],
                children: vec!["child1".to_string(), "child2".to_string()],
                types: vec!["TestType".to_string()],
            },
        );

        Self { plugins }
    }
}

/// Mock backend that generates IR based on configuration
pub struct ConfigurableBackend {
    config: BackendConfig,
}

impl ConfigurableBackend {
    /// Create a new backend with given configuration
    pub fn new(config: BackendConfig) -> Self {
        Self { config }
    }

    /// Generate IR based on current configuration
    pub fn generate_ir(&self) -> hub_codegen::IR {
        let mut ir_types = HashMap::new();
        let mut ir_methods = HashMap::new();
        let mut ir_plugins = HashMap::new();

        for (plugin_name, plugin_config) in &self.config.plugins {
            // Generate types for this plugin
            for type_name in &plugin_config.types {
                let full_name = format!("{}.{}", plugin_name, type_name);
                ir_types.insert(
                    full_name.clone(),
                    TypeDef {
                        td_name: type_name.clone(),
                        td_namespace: plugin_name.clone(),
                        td_description: Some(format!("Type {}", type_name)),
                        td_kind: TypeKind::KindStruct {
                            ks_fields: vec![FieldDef {
                                fd_name: "value".to_string(),
                                fd_type: TypeRef::RefPrimitive("string".to_string(), None),
                                fd_description: Some("Field value".to_string()),
                                fd_required: true,
                                fd_default: None, fd_deprecation: None,
                            }],
                        }, td_deprecation: None,},
                );
            }

            // Generate methods for this plugin
            for method_name in &plugin_config.methods {
                let full_path = format!("{}.{}", plugin_name, method_name);
                ir_methods.insert(
                    full_path.clone(),
                    MethodDef {
                        md_name: method_name.clone(),
                        md_full_path: full_path,
                        md_namespace: plugin_name.clone(),
                        md_description: Some(format!("Method {}", method_name)),
                        md_streaming: false,
                        md_params: vec![ParamDef {
                            pd_name: "input".to_string(),
                            pd_type: TypeRef::RefPrimitive("string".to_string(), None),
                            pd_description: Some("Input parameter".to_string()),
                            pd_required: true,
                            pd_default: None, pd_deprecation: None,
                        }],
                        md_returns: TypeRef::RefPrimitive("string".to_string(), None),
                        md_bidir_type: None,
                        md_role: Default::default(), md_deprecation: None,},
                );
            }

            // Add plugin mapping
            ir_plugins.insert(plugin_name.clone(), plugin_config.methods.clone());

            // Add child plugins as separate entries (simplified)
            for child_name in &plugin_config.children {
                let child_full_name = format!("{}.{}", plugin_name, child_name);
                ir_plugins.insert(
                    child_full_name.clone(),
                    vec!["child_method".to_string()],
                );
            }
        }

        // Compute global IR hash
        let ir_content = format!("{:?}{:?}{:?}", ir_types, ir_methods, ir_plugins);
        let ir_hash = compute_hash(&ir_content);

        hub_codegen::IR {
            ir_version: "2.0".to_string(),
            ir_backend: "configurable_test".to_string(),
            ir_hash: Some(ir_hash),
            ir_metadata: None,
            ir_types,
            ir_methods,
            ir_plugins, ir_plugin_deprecations: Default::default(),
        }
    }

    /// Compute self_hash (methods-only hash) for a plugin
    pub fn compute_self_hash(&self, plugin_name: &str) -> String {
        let ir = self.generate_ir();
        // Get methods for this plugin only (not children)
        let mut methods: Vec<_> = ir
            .ir_methods
            .iter()
            .filter(|(name, _)| {
                name.starts_with(&format!("{}.", plugin_name)) &&
                !name.contains(&format!("{}.", plugin_name).repeat(2)) // Not a child method
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Sort for deterministic hashing
        methods.sort_by(|a, b| a.0.cmp(&b.0));
        compute_hash(&format!("{:?}", methods))
    }

    /// Compute children_hash for a plugin
    pub fn compute_children_hash(&self, plugin_name: &str) -> String {
        let ir = self.generate_ir();
        // Get child plugin entries only
        let mut children: Vec<_> = ir
            .ir_plugins
            .keys()
            .filter(|name| {
                name.starts_with(&format!("{}.", plugin_name)) &&
                *name != plugin_name
            })
            .cloned()
            .collect();

        // Sort for deterministic hashing
        children.sort();
        compute_hash(&format!("{:?}", children))
    }

    /// Get plugin configuration
    pub fn get_plugin_config(&self, plugin_name: &str) -> Option<&PluginConfig> {
        self.config.plugins.get(plugin_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_backend_config_serialization() {
        println!("\n=== Backend Config Serialization ===");

        let config = BackendConfig::default_test_config();
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Save config
        config.save_to_file(&config_path).unwrap();
        println!("✅ Config saved to: {}", config_path.display());

        // Load config
        let loaded_config = BackendConfig::load_from_file(&config_path).unwrap();

        assert_eq!(loaded_config.plugins.len(), 1);
        assert!(loaded_config.plugins.contains_key("test_plugin"));

        let plugin = &loaded_config.plugins["test_plugin"];
        assert_eq!(plugin.methods.len(), 2);
        assert_eq!(plugin.children.len(), 2);

        println!("✅ Config loaded and validated");
    }

    #[test]
    fn test_configurable_backend_ir_generation() {
        println!("\n=== Configurable Backend IR Generation ===");

        let config = BackendConfig::default_test_config();
        let backend = ConfigurableBackend::new(config);

        let ir = backend.generate_ir();

        println!("Generated IR with {} plugins", ir.ir_plugins.len());
        println!("IR hash: {}", ir.ir_hash.as_ref().unwrap());

        // Verify IR structure
        assert_eq!(ir.ir_version, "2.0");
        assert_eq!(ir.ir_backend, "configurable_test");
        assert!(ir.ir_hash.is_some());

        // Check test_plugin
        assert!(ir.ir_plugins.contains_key("test_plugin"));
        let plugin_methods = &ir.ir_plugins["test_plugin"];
        assert_eq!(plugin_methods.len(), 2);
        assert!(plugin_methods.contains(&"method1".to_string()));
        assert!(plugin_methods.contains(&"method2".to_string()));

        println!("✅ IR structure validated");
    }

    #[test]
    fn test_scenario_a_method_only_change() {
        println!("\n=== Scenario A: Method-Only Change (Configurable Backend) ===");

        // Config 1: All methods
        let mut config1 = BackendConfig::default_test_config();
        config1.plugins.get_mut("test_plugin").unwrap().methods =
            vec!["method1".to_string(), "method2".to_string()];

        let backend1 = ConfigurableBackend::new(config1.clone());
        let self_hash_1 = backend1.compute_self_hash("test_plugin");
        let children_hash_1 = backend1.compute_children_hash("test_plugin");

        println!("Config 1 - self_hash:     {}", self_hash_1);
        println!("Config 1 - children_hash: {}", children_hash_1);

        // Config 2: Remove one method
        let mut config2 = config1.clone();
        config2.plugins.get_mut("test_plugin").unwrap().methods =
            vec!["method1".to_string()]; // Removed method2

        let backend2 = ConfigurableBackend::new(config2);
        let self_hash_2 = backend2.compute_self_hash("test_plugin");
        let children_hash_2 = backend2.compute_children_hash("test_plugin");

        println!("Config 2 - self_hash:     {}", self_hash_2);
        println!("Config 2 - children_hash: {}", children_hash_2);

        // Assertions
        assert_ne!(
            self_hash_1, self_hash_2,
            "self_hash should change when methods change"
        );
        assert_eq!(
            children_hash_1, children_hash_2,
            "children_hash should NOT change when only methods change"
        );

        println!("✅ Scenario A validated: Only self_hash changed");
    }

    #[test]
    fn test_scenario_b_children_only_change() {
        println!("\n=== Scenario B: Children-Only Change (Configurable Backend) ===");

        // Config 1: All children
        let mut config1 = BackendConfig::default_test_config();
        config1.plugins.get_mut("test_plugin").unwrap().children =
            vec!["child1".to_string(), "child2".to_string()];

        let backend1 = ConfigurableBackend::new(config1.clone());
        let self_hash_1 = backend1.compute_self_hash("test_plugin");
        let children_hash_1 = backend1.compute_children_hash("test_plugin");

        println!("Config 1 - self_hash:     {}", self_hash_1);
        println!("Config 1 - children_hash: {}", children_hash_1);

        // Config 2: Remove one child
        let mut config2 = config1.clone();
        config2.plugins.get_mut("test_plugin").unwrap().children =
            vec!["child1".to_string()]; // Removed child2

        let backend2 = ConfigurableBackend::new(config2);
        let self_hash_2 = backend2.compute_self_hash("test_plugin");
        let children_hash_2 = backend2.compute_children_hash("test_plugin");

        println!("Config 2 - self_hash:     {}", self_hash_2);
        println!("Config 2 - children_hash: {}", children_hash_2);

        // Assertions
        assert_eq!(
            self_hash_1, self_hash_2,
            "self_hash should NOT change when only children change"
        );
        assert_ne!(
            children_hash_1, children_hash_2,
            "children_hash should change when children change"
        );

        println!("✅ Scenario B validated: Only children_hash changed");
    }

    #[test]
    fn test_scenario_c_both_change() {
        println!("\n=== Scenario C: Both Methods and Children Change (Configurable Backend) ===");

        // Config 1: Initial state
        let config1 = BackendConfig::default_test_config();
        let backend1 = ConfigurableBackend::new(config1.clone());
        let self_hash_1 = backend1.compute_self_hash("test_plugin");
        let children_hash_1 = backend1.compute_children_hash("test_plugin");

        println!("Config 1 - self_hash:     {}", self_hash_1);
        println!("Config 1 - children_hash: {}", children_hash_1);

        // Config 2: Change both methods and children
        let mut config2 = config1.clone();
        let plugin_config = config2.plugins.get_mut("test_plugin").unwrap();
        plugin_config.methods = vec!["method1".to_string(), "method3".to_string()];
        plugin_config.children = vec!["child1".to_string(), "child3".to_string()];

        let backend2 = ConfigurableBackend::new(config2);
        let self_hash_2 = backend2.compute_self_hash("test_plugin");
        let children_hash_2 = backend2.compute_children_hash("test_plugin");

        println!("Config 2 - self_hash:     {}", self_hash_2);
        println!("Config 2 - children_hash: {}", children_hash_2);

        // Assertions
        assert_ne!(
            self_hash_1, self_hash_2,
            "self_hash should change when methods change"
        );
        assert_ne!(
            children_hash_1, children_hash_2,
            "children_hash should change when children change"
        );

        println!("✅ Scenario C validated: Both hashes changed");
    }

    #[test]
    fn test_multiple_plugins() {
        println!("\n=== Multiple Plugins Test ===");

        let mut config = BackendConfig {
            plugins: HashMap::new(),
        };

        // Add multiple plugins
        config.plugins.insert(
            "plugin_a".to_string(),
            PluginConfig {
                methods: vec!["method_a1".to_string()],
                children: vec![],
                types: vec!["TypeA".to_string()],
            },
        );

        config.plugins.insert(
            "plugin_b".to_string(),
            PluginConfig {
                methods: vec!["method_b1".to_string(), "method_b2".to_string()],
                children: vec!["child_b1".to_string()],
                types: vec!["TypeB".to_string()],
            },
        );

        let backend = ConfigurableBackend::new(config);
        let ir = backend.generate_ir();

        println!("Generated IR with {} plugin entries", ir.ir_plugins.len());

        // Verify both plugins exist
        assert!(ir.ir_plugins.contains_key("plugin_a"));
        assert!(ir.ir_plugins.contains_key("plugin_b"));

        // Compute hashes for each plugin
        let hash_a_self = backend.compute_self_hash("plugin_a");
        let hash_b_self = backend.compute_self_hash("plugin_b");

        println!("plugin_a self_hash: {}", hash_a_self);
        println!("plugin_b self_hash: {}", hash_b_self);

        assert_ne!(
            hash_a_self, hash_b_self,
            "Different plugins should have different hashes"
        );

        println!("✅ Multiple plugins validated");
    }

    #[test]
    fn test_empty_plugin() {
        println!("\n=== Empty Plugin Test ===");

        let mut config = BackendConfig {
            plugins: HashMap::new(),
        };

        config.plugins.insert(
            "empty_plugin".to_string(),
            PluginConfig {
                methods: vec![],
                children: vec![],
                types: vec![],
            },
        );

        let backend = ConfigurableBackend::new(config);
        let ir = backend.generate_ir();

        println!("Generated IR with empty plugin");

        // Plugin should exist but be empty
        assert!(ir.ir_plugins.contains_key("empty_plugin"));
        assert!(ir.ir_plugins["empty_plugin"].is_empty());

        let self_hash = backend.compute_self_hash("empty_plugin");
        println!("Empty plugin self_hash: {}", self_hash);
        assert_eq!(self_hash.len(), 16);

        println!("✅ Empty plugin validated");
    }
}
