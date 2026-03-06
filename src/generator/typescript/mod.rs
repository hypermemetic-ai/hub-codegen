//! TypeScript code generation from IR

pub mod types;
pub mod namespaces;
pub mod rpc;
pub mod transport;
pub mod package;
pub mod tests;

use crate::ir::IR;
use crate::generator::{GenerationResult, Warning, GenerationOptions, TransportEnv};
use crate::hash::compute_file_hashes;
use crate::HUB_CODEGEN_VERSION;
use anyhow::Result;
use std::collections::HashMap;
use serde_json::json;

/// Generate TypeScript code from IR
pub fn generate(ir: &IR, options: &GenerationOptions) -> Result<GenerationResult> {
    let mut files = HashMap::new();
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

    // Generate core types (PlexusStreamItem etc) - shared across all namespaces
    let core_types_content = types::generate_types(ir);
    files.insert("types.ts".to_string(), core_types_content);

    // Generate RPC client interface (Layer 1)
    let rpc_content = rpc::generate_rpc_client();
    files.insert("rpc.ts".to_string(), rpc_content);

    // Generate per-namespace type files (<namespace>/types.ts)
    let namespace_type_files = types::generate_namespace_types(ir);
    files.extend(namespace_type_files);

    // Generate namespace client files (<namespace>/client.ts and <namespace>/index.ts)
    let namespace_files = namespaces::generate_namespaces(ir);
    files.extend(namespace_files);

    // Generate WebSocket transport implementation (Layer 1 implementation)
    // Only if transport is not None
    if options.transport != TransportEnv::None {
        let transport_content = transport::generate_transport(options.transport);
        files.insert("transport.ts".to_string(), transport_content);
    }

    // Check if bidirectional methods exist (for package.json scripts)
    let has_bidir = tests::has_bidir_methods(ir);

    // Generate index
    let index = generate_index(ir, options.transport);
    files.insert("index.ts".to_string(), index);

    // Generate package.json — included in files so synapse-cc's three-way merge
    // can update it when the generated content changes (e.g. test script changed).
    let pkg_json = package::generate_package_json(ir, options.transport, has_bidir);
    files.insert("package.json".to_string(), pkg_json);

    // Generate tsconfig.json
    let tsconfig = package::generate_tsconfig(options.transport);
    files.insert("tsconfig.json".to_string(), tsconfig);

    // Generate smoke test
    let smoke_test = tests::generate_smoke_test(ir, options.transport);
    files.insert("test/smoke.test.ts".to_string(), smoke_test);

    // Generate bidirectional smoke test if bidir methods exist
    if has_bidir {
        let bidir_test = tests::generate_bidir_smoke_test(ir, options.transport);
        files.insert("test/bidir-smoke.test.ts".to_string(), bidir_test);
    }

    // Compute file hashes for all files generated so far
    let mut file_hashes = compute_file_hashes(&files);

    // Generate metadata file with file hashes included
    let metadata = generate_metadata_file(ir, &file_hashes);
    files.insert(".codegen-metadata.json".to_string(), metadata.clone());

    // Compute hash for metadata file itself
    use crate::hash::compute_file_hash;
    file_hashes.insert(".codegen-metadata.json".to_string(), compute_file_hash(&metadata));

    let dependencies = package::get_runtime_deps(options.transport);
    let dev_dependencies = package::get_dev_deps(options.transport);

    Ok(GenerationResult { files, warnings, file_hashes, dependencies, dev_dependencies })
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
