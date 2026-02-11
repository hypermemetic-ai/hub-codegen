//! Example: Generate IR from a configuration file
//!
//! Usage:
//!   cargo run --example generate_from_config tests/test_scenarios/scenario_a_initial.json

use std::env;
use std::fs;
use std::path::Path;

// Re-export the configurable backend test module
// In a real setup, this would be a proper module, not a test module
// For now, we'll implement a simple version here

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackendConfig {
    plugins: HashMap<String, PluginConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PluginConfig {
    methods: Vec<String>,
    #[serde(default)]
    children: Vec<String>,
    #[serde(default)]
    types: Vec<String>,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <config.json>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} tests/test_scenarios/scenario_a_initial.json", args[0]);
        std::process::exit(1);
    }

    let config_path = Path::new(&args[1]);

    if !config_path.exists() {
        eprintln!("Error: Config file not found: {}", config_path.display());
        std::process::exit(1);
    }

    println!("Loading configuration from: {}", config_path.display());

    // Load config
    let content = fs::read_to_string(config_path)
        .expect("Failed to read config file");

    let config: BackendConfig = serde_json::from_str(&content)
        .expect("Failed to parse config JSON");

    println!("\n=== Configuration ===");
    println!("Plugins: {}", config.plugins.len());

    for (plugin_name, plugin_config) in &config.plugins {
        println!("\nPlugin: {}", plugin_name);
        println!("  Methods: {}", plugin_config.methods.len());
        for method in &plugin_config.methods {
            println!("    - {}", method);
        }
        println!("  Children: {}", plugin_config.children.len());
        for child in &plugin_config.children {
            println!("    - {}", child);
        }
        println!("  Types: {}", plugin_config.types.len());
        for type_name in &plugin_config.types {
            println!("    - {}", type_name);
        }
    }

    println!("\n=== Hash Computation ===");

    // Compute hashes for each plugin
    for (plugin_name, plugin_config) in &config.plugins {
        let methods_hash = compute_simple_hash(&format!("{:?}", plugin_config.methods));
        let children_hash = compute_simple_hash(&format!("{:?}", plugin_config.children));
        let composite_hash = compute_simple_hash(&format!(
            "{:?}{:?}",
            plugin_config.methods, plugin_config.children
        ));

        println!("\nPlugin: {}", plugin_name);
        println!("  self_hash (methods):   {}", methods_hash);
        println!("  children_hash:         {}", children_hash);
        println!("  composite_hash:        {}", composite_hash);
    }

    println!("\n✅ Configuration processed successfully");
}

// Simple hash function for demonstration
fn compute_simple_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:016x}", hash)
}
