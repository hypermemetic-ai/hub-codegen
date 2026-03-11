//! TypeScript code generation from IR

pub mod types;
pub mod namespaces;
pub mod rpc;
pub mod transport;
pub mod package;
pub mod tests;

use crate::ir::IR;
use crate::generator::{GenerationResult, Warning, GenerationOptions, GenerateSelector, TransportEnv};
use crate::hash::compute_file_hashes;
use crate::HUB_CODEGEN_VERSION;
use anyhow::Result;
use std::collections::HashMap;
use serde_json::json;

/// Generate TypeScript code from IR
pub fn generate(ir: &IR, options: &GenerationOptions) -> Result<GenerationResult> {
    let mut warnings = Vec::new();

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

    // Dispatch to artifact-specific generator
    let mut files = match options.generate {
        GenerateSelector::All      => generate_all(ir, options),
        GenerateSelector::Transport => generate_transport_only(ir, options),
        GenerateSelector::Rpc      => generate_rpc_only(ir, options),
        GenerateSelector::Plugins  => generate_plugins_only(ir, options),
        GenerateSelector::Smoke    => generate_smoke_only(ir, options),
        GenerateSelector::Package  => generate_package_only(ir, options),
    };

    // Compute file hashes
    let mut file_hashes = compute_file_hashes(&files);

    // Metadata only for GenAll (other selectors produce partial outputs)
    if options.generate == GenerateSelector::All {
        let metadata = generate_metadata_file(ir, &file_hashes);
        files.insert(".codegen-metadata.json".to_string(), metadata.clone());
        use crate::hash::compute_file_hash;
        file_hashes.insert(".codegen-metadata.json".to_string(), compute_file_hash(&metadata));
    }

    let dependencies = package::get_runtime_deps(options.transport);
    let dev_dependencies = package::get_dev_deps(options.transport);

    Ok(GenerationResult { files, warnings, file_hashes, dependencies, dev_dependencies })
}

/// GenAll: all artifacts (current behaviour)
fn generate_all(ir: &IR, options: &GenerationOptions) -> HashMap<String, String> {
    let mut files = HashMap::new();

    files.insert("types.ts".to_string(), types::generate_types(ir));
    files.insert("rpc.ts".to_string(), rpc::generate_rpc_client());
    files.extend(types::generate_namespace_types(ir, None));
    files.extend(namespaces::generate_namespaces(ir, None));

    if options.transport != TransportEnv::None {
        files.insert("transport.ts".to_string(), transport::generate_transport(options.transport));
    }

    let has_bidir = tests::has_bidir_methods(ir);
    files.insert("index.ts".to_string(), generate_index(ir, options.transport));
    files.insert("package.json".to_string(), package::generate_package_json(ir, options.transport, has_bidir));
    files.insert("tsconfig.json".to_string(), package::generate_tsconfig(options.transport));
    files.insert("test/smoke.test.ts".to_string(), tests::generate_smoke_test(ir, options.transport, &options.backend_url));

    if has_bidir {
        files.insert("test/bidir-smoke.test.ts".to_string(), tests::generate_bidir_smoke_test(ir, options.transport, &options.backend_url));
    }

    files
}

/// GenTransport: transport.ts only
fn generate_transport_only(_ir: &IR, options: &GenerationOptions) -> HashMap<String, String> {
    if options.transport == TransportEnv::None {
        return HashMap::new();
    }
    let mut files = HashMap::new();
    files.insert("transport.ts".to_string(), transport::generate_transport(options.transport));
    files
}

/// GenRpc: core RPC layer — types.ts, rpc.ts, index.ts
fn generate_rpc_only(ir: &IR, options: &GenerationOptions) -> HashMap<String, String> {
    let mut files = HashMap::new();
    files.insert("types.ts".to_string(), types::generate_types(ir));
    files.insert("rpc.ts".to_string(), rpc::generate_rpc_client());
    files.insert("index.ts".to_string(), generate_index(ir, options.transport));
    files
}

/// GenPlugins: namespace client files with optional plugin filter.
///
/// The filter is applied before generation: only matching namespaces are
/// generated, rather than generating all then discarding.
fn generate_plugins_only(ir: &IR, options: &GenerationOptions) -> HashMap<String, String> {
    let filter = options.plugins_filter.as_deref();
    let mut files = HashMap::new();
    files.extend(types::generate_namespace_types(ir, filter));
    files.extend(namespaces::generate_namespaces(ir, filter));
    files
}

/// GenSmoke: schema walk smoke test (no test framework)
fn generate_smoke_only(ir: &IR, options: &GenerationOptions) -> HashMap<String, String> {
    let mut files = HashMap::new();
    let content = tests::generate_schema_walk_smoke(ir, options.transport, &options.smoke_transport_path);
    files.insert("smoke.ts".to_string(), content);
    files
}

/// GenPackage: package.json only
fn generate_package_only(ir: &IR, options: &GenerationOptions) -> HashMap<String, String> {
    let has_bidir = tests::has_bidir_methods(ir);
    let mut files = HashMap::new();
    files.insert("package.json".to_string(), package::generate_package_json(ir, options.transport, has_bidir));
    files
}

/// Generate .codegen-metadata.json with full toolchain information
fn generate_metadata_file(ir: &IR, file_hashes: &HashMap<String, String>) -> String {
    use crate::ir::GeneratorInfo;

    // Get generators from IR metadata and add hub-codegen itself
    let mut generators = ir.ir_metadata
        .as_ref()
        .map(|m| m.gm_generators.clone())
        .unwrap_or_default();

    generators.push(GeneratorInfo {
        gi_tool: "hub-codegen".to_string(),
        gi_version: HUB_CODEGEN_VERSION.to_string(),
    });

    let metadata = json!({
        "format_version": "2.0",
        "generation": {
            "toolchain": generators.iter().map(|g| json!({
                "tool": g.gi_tool,
                "version": g.gi_version,
            })).collect::<Vec<_>>(),
            "timestamp": ir.ir_metadata.as_ref().map(|m| &m.gm_timestamp),
            "ir_version": &ir.ir_version,
        },
        "source": {
            "backend": &ir.ir_backend,
            "plexus_hash": &ir.ir_hash,
        },
        "cache": {
            "file_hashes": file_hashes,
        },
    });

    serde_json::to_string_pretty(&metadata).unwrap()
}

/// Collect warnings for unknown/untyped references
///
/// Only warns on RefUnknown (schema gaps), NOT on RefAny (intentionally dynamic)
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

fn generate_index(ir: &IR, transport: TransportEnv) -> String {
    let mut lines = vec![
        "// Auto-generated by hub-codegen".to_string(),
        "".to_string(),
        "// Core types (PlexusStreamItem, etc)".to_string(),
        "export * from './types';".to_string(),
        "".to_string(),
        "// RPC client interface (Layer 1)".to_string(),
        "export * from './rpc';".to_string(),
        "".to_string(),
    ];

    if transport != TransportEnv::None {
        lines.push("// WebSocket transport (Layer 1 implementation)".to_string());
        lines.push("export * from './transport';".to_string());
        lines.push("".to_string());
    }

    lines.push("// Namespace modules (types + clients)".to_string());

    // Get unique namespaces
    let mut namespaces: Vec<_> = ir.ir_plugins.keys().collect();
    namespaces.sort();

    for namespace in namespaces {
        // Skip empty namespace (core plexus methods)
        if namespace.is_empty() {
            continue;
        }

        // Export namespace as a module
        // e.g., "hyperforge.workspace.repos" → "export * as HyperforgeWorkspaceRepos from './hyperforge/workspace/repos';"
        let pascal_name = to_pascal(namespace);
        let path = namespace.replace('.', "/");
        lines.push(format!("export * as {} from './{}';", pascal_name, path));
    }

    lines.join("\n")
}

fn to_pascal(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for c in s.chars() {
        if c == '_' || c == '-' || c == '.' {  // Treat dots as word boundaries
            capitalize = true;
        } else if capitalize {
            result.push(c.to_ascii_uppercase());
            capitalize = false;
        } else {
            result.push(c);
        }
    }
    result
}
