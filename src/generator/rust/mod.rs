//! Rust code generation from IR

pub mod types;
pub mod client;

#[cfg(test)]
mod tests;

use crate::ir::IR;
use crate::generator::{GenerationResult, Warning};
use crate::deprecation::{self, DeprecationOptions, DeprecationWarning};
use crate::hash::compute_file_hashes;
use anyhow::Result;
use std::collections::HashMap;

/// Parse namespace into path segments
/// "hyperforge.org.hypermemetic" -> ["hyperforge", "org", "hypermemetic"]
fn parse_namespace_path(namespace: &str) -> Vec<String> {
    if namespace.is_empty() {
        vec![]
    } else {
        namespace.split('.').map(|s| s.to_string()).collect()
    }
}

/// Convert namespace path to module file path
/// ["hyperforge", "org", "hypermemetic"] -> "src/hyperforge/org/hypermemetic/mod.rs"
fn namespace_to_file_path(path: &[String]) -> String {
    if path.is_empty() {
        "src/mod.rs".to_string()
    } else {
        format!("src/{}/mod.rs", path.join("/"))
    }
}

/// Generate Rust code from IR (legacy entry — deprecation annotations
/// disabled by default to match pre-IR-7 callers).
pub fn generate(ir: &IR) -> Result<GenerationResult> {
    generate_with_options(ir, DeprecationOptions { enabled: false })
}

/// Generate Rust code from IR with explicit deprecation toggle (IR-7).
pub fn generate_with_options(ir: &IR, deprecation_opts: DeprecationOptions) -> Result<GenerationResult> {
    let mut files = HashMap::new();
    let mut warnings = Vec::new();
    let mut deprecation_warnings: Vec<DeprecationWarning> = Vec::new();

    // Validate IR version
    if ir.ir_version != "2.0" {
        anyhow::bail!(
            "Unsupported IR version: {}. Expected 2.0.\n\
             This version of hub-codegen requires IR v2.0 with structured TypeRef.\n\
             Please regenerate IR with latest Synapse.",
            ir.ir_version
        );
    }

    // Collect warnings for unknown types
    collect_warnings(ir, &mut warnings);

    let emit_deprecation = deprecation_opts.enabled && deprecation::is_post_ir(ir);

    // Generate lib.rs (re-exports all modules)
    let lib_content = generate_lib(ir);
    files.insert("src/lib.rs".to_string(), lib_content);

    // Generate core transport types only
    let types_content = types::generate_core_types(ir);
    files.insert("src/types.rs".to_string(), types_content);

    // Generate base client struct and transport (R-4: appends
    // credential-requirement metadata types when the IR surfaces any).
    let client_content = client::generate_base_client_with_ir(ir);
    files.insert("src/client.rs".to_string(), client_content);

    // Generate namespace modules (methods + types, one file per namespace)
    let namespace_files = client::generate_namespace_modules_with_deprecation(
        ir,
        emit_deprecation,
        &mut deprecation_warnings,
    );
    files.extend(namespace_files);

    // Generate Cargo.toml
    let cargo_toml = generate_cargo_toml(ir);
    files.insert("Cargo.toml".to_string(), cargo_toml);

    Ok(GenerationResult {
        file_hashes: compute_file_hashes(&files),
        files,
        warnings,
        dependencies: HashMap::new(),
        dev_dependencies: HashMap::new(),
        deprecation_warnings,
    })
}

/// Collect warnings for unknown/untyped references
fn collect_warnings(ir: &IR, warnings: &mut Vec<Warning>) {
    use crate::ir::TypeKind;

    // Check method return types and params
    for (path, method) in &ir.ir_methods {
        if method.md_returns.is_unknown() {
            warnings.push(Warning {
                location: path.clone(),
                message: "return type is unknown (missing schema)".to_string(),
            });
        }

        for param in &method.md_params {
            if param.pd_type.is_unknown() {
                warnings.push(Warning {
                    location: format!("{}({})", path, param.pd_name),
                    message: "parameter type is unknown (missing schema)".to_string(),
                });
            }
        }
    }

    // Check type definitions for unknown field types
    for (name, typedef) in &ir.ir_types {
        match &typedef.td_kind {
            TypeKind::KindStruct { ks_fields } => {
                for field in ks_fields {
                    if field.fd_type.contains_unknown() {
                        warnings.push(Warning {
                            location: format!("{}.{}", name, field.fd_name),
                            message: "field type contains unknown (missing schema)".to_string(),
                        });
                    }
                }
            }
            TypeKind::KindEnum { ke_variants, .. } => {
                for variant in ke_variants {
                    for field in &variant.vd_fields {
                        if field.fd_type.contains_unknown() {
                            warnings.push(Warning {
                                location: format!("{}.{}.{}", name, variant.vd_name, field.fd_name),
                                message: "field type contains unknown (missing schema)".to_string(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn generate_lib(ir: &IR) -> String {
    let mut lines = vec![
        "//! Auto-generated Plexus client".to_string(),
        "//! Do not edit manually".to_string(),
        "".to_string(),
        "pub mod types;".to_string(),
        "pub mod client;".to_string(),
        "".to_string(),
    ];

    // Build namespace tree to find top-level modules
    let root = client::build_namespace_tree(ir);

    // Declare only top-level namespace modules
    let mut top_level: Vec<_> = root.children.keys().collect();
    top_level.sort();

    if !top_level.is_empty() {
        lines.push("// Top-level namespace modules".to_string());
        for name in top_level {
            lines.push(format!("pub mod {};", name));
        }
    }

    lines.push("".to_string());
    lines.push("pub use client::PlexusClient;".to_string());
    lines.push("".to_string());
    lines.push("// Re-export common types".to_string());
    lines.push("pub use types::PlexusStreamItem;".to_string());

    lines.join("\n")
}

fn generate_cargo_toml(ir: &IR) -> String {
    // Always use 0.1.0 for version - hash is metadata, not a semver version
    let version = "0.1.0";
    let metadata = ir.ir_hash.as_deref().map(|h| format!(" (hash: {})", h)).unwrap_or_default();

    format!(
        r#"[package]
name = "plexus-client"
version = "{}"
edition = "2021"
description = "Auto-generated Plexus client{}"

[dependencies]
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
tokio = {{ version = "1.0", features = ["full"] }}
tokio-tungstenite = "0.21"
futures = "0.3"
anyhow = "1.0"
async-stream = "0.3"
thiserror = "1.0"

[dev-dependencies]
tokio-test = "0.4"
"#,
        version, metadata
    )
}
