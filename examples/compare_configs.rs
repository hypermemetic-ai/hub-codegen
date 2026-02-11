//! Example: Compare two configuration files and show hash differences
//!
//! Usage:
//!   cargo run --example compare_configs config1.json config2.json

use std::env;
use std::fs;
use std::path::Path;

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

    if args.len() < 3 {
        eprintln!("Usage: {} <config1.json> <config2.json>", args[0]);
        eprintln!("\nExample:");
        eprintln!(
            "  {} tests/test_scenarios/scenario_a_initial.json tests/test_scenarios/scenario_a_modified.json",
            args[0]
        );
        std::process::exit(1);
    }

    let config1_path = Path::new(&args[1]);
    let config2_path = Path::new(&args[2]);

    // Load configs
    let config1 = load_config(config1_path);
    let config2 = load_config(config2_path);

    println!("\n=== Configuration Comparison ===");
    println!("Config 1: {}", config1_path.display());
    println!("Config 2: {}", config2_path.display());

    // Compare each plugin
    for plugin_name in config1.plugins.keys() {
        if !config2.plugins.contains_key(plugin_name) {
            println!("\n⚠️  Plugin '{}' exists in config1 but not in config2", plugin_name);
            continue;
        }

        let plugin1 = &config1.plugins[plugin_name];
        let plugin2 = &config2.plugins[plugin_name];

        println!("\n=== Plugin: {} ===", plugin_name);

        // Compare methods
        let methods1_hash = compute_simple_hash(&format!("{:?}", plugin1.methods));
        let methods2_hash = compute_simple_hash(&format!("{:?}", plugin2.methods));

        println!("\nMethods:");
        println!("  Config 1: {} methods, hash: {}", plugin1.methods.len(), methods1_hash);
        println!("  Config 2: {} methods, hash: {}", plugin2.methods.len(), methods2_hash);

        if methods1_hash != methods2_hash {
            println!("  ❌ CHANGED - self_hash will be invalidated");
            show_diff("Methods", &plugin1.methods, &plugin2.methods);
        } else {
            println!("  ✅ UNCHANGED");
        }

        // Compare children
        let children1_hash = compute_simple_hash(&format!("{:?}", plugin1.children));
        let children2_hash = compute_simple_hash(&format!("{:?}", plugin2.children));

        println!("\nChildren:");
        println!("  Config 1: {} children, hash: {}", plugin1.children.len(), children1_hash);
        println!("  Config 2: {} children, hash: {}", plugin2.children.len(), children2_hash);

        if children1_hash != children2_hash {
            println!("  ❌ CHANGED - children_hash will be invalidated");
            show_diff("Children", &plugin1.children, &plugin2.children);
        } else {
            println!("  ✅ UNCHANGED");
        }

        // Compare types
        let types1_hash = compute_simple_hash(&format!("{:?}", plugin1.types));
        let types2_hash = compute_simple_hash(&format!("{:?}", plugin2.types));

        println!("\nTypes:");
        println!("  Config 1: {} types, hash: {}", plugin1.types.len(), types1_hash);
        println!("  Config 2: {} types, hash: {}", plugin2.types.len(), types2_hash);

        if types1_hash != types2_hash {
            println!("  ❌ CHANGED - self_hash will be invalidated");
            show_diff("Types", &plugin1.types, &plugin2.types);
        } else {
            println!("  ✅ UNCHANGED");
        }

        // Summary
        println!("\n=== Cache Invalidation Impact ===");
        if methods1_hash != methods2_hash || types1_hash != types2_hash {
            println!("  ⚠️  self_hash will change → Regenerate method bindings");
        } else {
            println!("  ✅ self_hash unchanged → Reuse cached method bindings");
        }

        if children1_hash != children2_hash {
            println!("  ⚠️  children_hash will change → Regenerate child bindings");
        } else {
            println!("  ✅ children_hash unchanged → Reuse cached child bindings");
        }
    }

    // Check for plugins in config2 but not in config1
    for plugin_name in config2.plugins.keys() {
        if !config1.plugins.contains_key(plugin_name) {
            println!("\n⚠️  Plugin '{}' exists in config2 but not in config1", plugin_name);
        }
    }
}

fn load_config(path: &Path) -> BackendConfig {
    if !path.exists() {
        eprintln!("Error: Config file not found: {}", path.display());
        std::process::exit(1);
    }

    let content = fs::read_to_string(path)
        .expect("Failed to read config file");

    serde_json::from_str(&content)
        .expect("Failed to parse config JSON")
}

fn compute_simple_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:016x}", hash)
}

fn show_diff(label: &str, list1: &[String], list2: &[String]) {
    let set1: std::collections::HashSet<_> = list1.iter().collect();
    let set2: std::collections::HashSet<_> = list2.iter().collect();

    let added: Vec<_> = set2.difference(&set1).collect();
    let removed: Vec<_> = set1.difference(&set2).collect();

    if !removed.is_empty() {
        println!("    Removed {}:", label);
        for item in removed {
            println!("      - {}", item);
        }
    }

    if !added.is_empty() {
        println!("    Added {}:", label);
        for item in added {
            println!("      + {}", item);
        }
    }
}
