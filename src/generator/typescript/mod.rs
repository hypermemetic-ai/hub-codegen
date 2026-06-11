//! TypeScript code generation from IR

pub mod types;
pub mod namespaces;
pub mod rpc;
pub mod transport;
pub mod package;
pub mod tests;

use crate::ir::{IR, TypeRef, TypeKind};
use crate::generator::{GenerationResult, Warning, GenerationOptions, GenerateSelector, TransportEnv};
use crate::hash::{compute_file_hashes, compute_plugin_hash};
use crate::deprecation::{self, DeprecationWarning};
use crate::HUB_CODEGEN_VERSION;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use serde_json::json;

/// Result of partitioning plugins into requested and type-dependency-only sets.
#[derive(Debug, Clone)]
struct PluginPartition {
    /// Plugins explicitly requested — get full generation (client.ts, types.ts, index.ts)
    requested: HashSet<String>,
    /// Namespaces not requested but needed for cross-plugin type references — types.ts only
    type_deps: HashSet<String>,
}

impl PluginPartition {
    /// Returns true if the namespace should get types.ts generated (either requested or dep)
    #[allow(dead_code)]
    fn needs_types(&self, ns: &str) -> bool {
        self.requested.contains(ns) || self.type_deps.contains(ns)
    }

    /// Returns true if the namespace should get client.ts + index.ts (only requested plugins)
    #[allow(dead_code)]
    fn needs_client(&self, ns: &str) -> bool {
        self.requested.contains(ns)
    }
}

/// Resolve type dependencies transitively from a set of requested plugin namespaces.
///
/// Walks all `RefNamed` type references from types belonging to requested plugins.
/// If a ref points to a namespace not in `requested`, that namespace is added to
/// `type_deps`. Then we transitively resolve — dependency types may reference further
/// namespaces. Repeats until stable (fixed point).
fn resolve_type_dependencies(ir: &IR, requested: &[String]) -> PluginPartition {
    let requested_set: HashSet<String> = requested.iter().cloned().collect();
    let mut type_deps: HashSet<String> = HashSet::new();
    let mut frontier: HashSet<String> = requested_set.clone();
    let mut visited: HashSet<String> = HashSet::new();

    loop {
        let mut new_deps: HashSet<String> = HashSet::new();

        for ns in &frontier {
            if visited.contains(ns) {
                continue;
            }
            visited.insert(ns.clone());

            // Walk all types in this namespace and collect referenced namespaces
            for typedef in ir.ir_types.values() {
                if typedef.td_namespace != *ns {
                    continue;
                }
                collect_ns_refs_from_kind(&typedef.td_kind, &mut new_deps);
            }

            // Also walk method params/returns for this namespace
            for method in ir.ir_methods.values() {
                if method.md_namespace != *ns {
                    continue;
                }
                collect_ns_refs_from_typeref(&method.md_returns, &mut new_deps);
                for param in &method.md_params {
                    collect_ns_refs_from_typeref(&param.pd_type, &mut new_deps);
                }
                if let Some(bidir) = &method.md_bidir_type {
                    collect_ns_refs_from_typeref(bidir, &mut new_deps);
                }
            }
        }

        // Remove already-known namespaces
        new_deps.retain(|ns| !ns.is_empty() && !requested_set.contains(ns) && !type_deps.contains(ns));

        if new_deps.is_empty() {
            break;
        }

        // The new deps become the next frontier for transitive resolution
        frontier = new_deps.clone();
        type_deps.extend(new_deps);
    }

    PluginPartition {
        requested: requested_set,
        type_deps,
    }
}

/// Collect namespace references from a TypeRef
fn collect_ns_refs_from_typeref(tr: &TypeRef, namespaces: &mut HashSet<String>) {
    match tr {
        TypeRef::RefNamed(qn) => {
            if let Some(ns) = qn.namespace() {
                namespaces.insert(ns.to_string());
            }
        }
        TypeRef::RefArray(inner) => collect_ns_refs_from_typeref(inner, namespaces),
        TypeRef::RefOptional(inner) => collect_ns_refs_from_typeref(inner, namespaces),
        _ => {}
    }
}

/// Collect namespace references from a TypeKind (struct fields, enum variants, aliases)
fn collect_ns_refs_from_kind(kind: &TypeKind, namespaces: &mut HashSet<String>) {
    match kind {
        TypeKind::KindStruct { ks_fields } => {
            for field in ks_fields {
                collect_ns_refs_from_typeref(&field.fd_type, namespaces);
            }
        }
        TypeKind::KindEnum { ke_variants, .. } => {
            for variant in ke_variants {
                for field in &variant.vd_fields {
                    collect_ns_refs_from_typeref(&field.fd_type, namespaces);
                }
            }
        }
        TypeKind::KindAlias { ka_target } => {
            collect_ns_refs_from_typeref(ka_target, namespaces);
        }
        TypeKind::KindPrimitive { .. } | TypeKind::KindStringEnum { .. } => {}
    }
}

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

    // IR-7: resolve effective deprecation toggle. Post-IR + enabled → emit.
    let emit_deprecation = options.deprecation.enabled && deprecation::is_post_ir(ir);
    let mut deprecation_warnings: Vec<DeprecationWarning> = Vec::new();

    // Dispatch to artifact-specific generator
    let mut files = match options.generate {
        GenerateSelector::All      => generate_all(ir, options, emit_deprecation, &mut deprecation_warnings),
        GenerateSelector::Transport => generate_transport_only(ir, options),
        GenerateSelector::Rpc      => generate_rpc_only(ir, options, emit_deprecation, &mut deprecation_warnings),
        GenerateSelector::Plugins  => generate_plugins_only(ir, options, emit_deprecation, &mut deprecation_warnings),
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

    Ok(GenerationResult { files, warnings, file_hashes, dependencies, dev_dependencies, deprecation_warnings })
}

/// Generate all code files except package.json.
/// Used both by generate_all and generate_package_only to compute a
/// content-stable version hash before generating package.json.
fn generate_code_files(
    ir: &IR,
    options: &GenerationOptions,
    emit_deprecation: bool,
    deprecation_warnings: &mut Vec<DeprecationWarning>,
) -> HashMap<String, String> {
    let mut files = HashMap::new();

    files.insert("types.ts".to_string(), types::generate_types(ir));
    files.insert("rpc.ts".to_string(), rpc::generate_rpc_client());

    // When --plugins is specified, partition namespaces into requested (full gen)
    // and type-dependency-only (types.ts stubs). Otherwise generate everything.
    let partition = options.plugins_filter.as_ref().map(|pf| resolve_type_dependencies(ir, pf));

    if let Some(ref part) = partition {
        // Generate types for requested plugins AND their type dependencies
        let all_type_ns: Vec<String> = part.requested.iter()
            .chain(part.type_deps.iter())
            .cloned()
            .collect();
        files.extend(types::generate_namespace_types(ir, Some(&all_type_ns), emit_deprecation, deprecation_warnings));

        // Generate client + index only for requested plugins
        let requested_vec: Vec<String> = part.requested.iter().cloned().collect();
        files.extend(namespaces::generate_namespaces(ir, Some(&requested_vec), emit_deprecation, deprecation_warnings));
    } else {
        files.extend(types::generate_namespace_types(ir, None, emit_deprecation, deprecation_warnings));
        files.extend(namespaces::generate_namespaces(ir, None, emit_deprecation, deprecation_warnings));
    }

    if options.transport != TransportEnv::None {
        files.insert("transport.ts".to_string(), transport::generate_transport(options.transport, ir));
    }

    // Top-level index.ts only re-exports requested plugins (or all if no filter)
    let index_filter = partition.as_ref().map(|p| &p.requested);
    files.insert("index.ts".to_string(), generate_index_filtered(ir, options.transport, index_filter));
    files.insert("tsconfig.json".to_string(), package::generate_tsconfig(options.transport));
    files.insert("test/smoke.test.ts".to_string(), tests::generate_smoke_test(ir, options.transport, &options.backend_url));

    if tests::has_bidir_methods(ir) {
        files.insert("test/bidir-smoke.test.ts".to_string(), tests::generate_bidir_smoke_test(ir, options.transport, &options.backend_url));
    }

    files
}

/// GenAll: all artifacts (current behaviour)
fn generate_all(
    ir: &IR,
    options: &GenerationOptions,
    emit_deprecation: bool,
    deprecation_warnings: &mut Vec<DeprecationWarning>,
) -> HashMap<String, String> {
    // Two-pass: generate code files first, derive a content-stable version hash,
    // then generate package.json. This ensures package.json's version only changes
    // when generated code changes — not when IR metadata (timestamps, unrelated
    // plugin additions) changes.
    let mut files = generate_code_files(ir, options, emit_deprecation, deprecation_warnings);
    let version_hash = compute_plugin_hash(&files);
    let has_bidir = tests::has_bidir_methods(ir);
    files.insert("package.json".to_string(), package::generate_package_json(options.transport, has_bidir, &version_hash));
    files
}

/// GenTransport: protocol types + RPC helpers + WebSocket transport.
/// types.ts and rpc.ts are static; since CA-2 the transport carries the
/// IR-derived method-auth registry (empty registry for surface-free IRs).
fn generate_transport_only(ir: &IR, options: &GenerationOptions) -> HashMap<String, String> {
    if options.transport == TransportEnv::None {
        return HashMap::new();
    }
    let mut files = HashMap::new();
    files.insert("types.ts".to_string(), types::generate_protocol_types());
    files.insert("rpc.ts".to_string(), rpc::generate_rpc_client());
    files.insert("transport.ts".to_string(), transport::generate_transport(options.transport, ir));
    files
}

/// GenRpc: core RPC layer — types.ts, rpc.ts, index.ts
fn generate_rpc_only(
    ir: &IR,
    options: &GenerationOptions,
    _emit_deprecation: bool,
    _deprecation_warnings: &mut Vec<DeprecationWarning>,
) -> HashMap<String, String> {
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
fn generate_plugins_only(
    ir: &IR,
    options: &GenerationOptions,
    emit_deprecation: bool,
    deprecation_warnings: &mut Vec<DeprecationWarning>,
) -> HashMap<String, String> {
    let filter = options.plugins_filter.as_deref();
    let mut files = HashMap::new();
    files.extend(types::generate_namespace_types(ir, filter, emit_deprecation, deprecation_warnings));
    files.extend(namespaces::generate_namespaces(ir, filter, emit_deprecation, deprecation_warnings));
    files
}

/// GenSmoke: schema walk smoke test (no test framework)
fn generate_smoke_only(ir: &IR, options: &GenerationOptions) -> HashMap<String, String> {
    let mut files = HashMap::new();
    let content = tests::generate_schema_walk_smoke(ir, options.transport, &options.smoke_transport_path);
    files.insert("smoke.ts".to_string(), content);
    files
}

/// GenPackage: package.json only.
/// Generates all code files in memory to compute the content-stable version hash,
/// then returns only package.json.
fn generate_package_only(ir: &IR, options: &GenerationOptions) -> HashMap<String, String> {
    // IR-7: Do not surface deprecation warnings from the throwaway hash pass —
    // they would double-count when generate_all later emits the same files.
    // `emit_deprecation` stays live because the hash must reflect the real
    // generated content (which includes annotations when post-IR).
    let emit_deprecation = options.deprecation.enabled && deprecation::is_post_ir(ir);
    let mut scratch = Vec::new();
    let code_files = generate_code_files(ir, options, emit_deprecation, &mut scratch);
    let version_hash = compute_plugin_hash(&code_files);
    let has_bidir = tests::has_bidir_methods(ir);
    let mut files = HashMap::new();
    files.insert("package.json".to_string(), package::generate_package_json(options.transport, has_bidir, &version_hash));
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

    // Deliberately omit timestamp and plexus_hash — both change on every build
    // even when code content is unchanged, causing spurious file-hash churn.
    // The IR hash is tracked in synapse.lock (irHash field); build timestamps
    // are not useful in committed generated files.
    let metadata = json!({
        "format_version": "2.0",
        "generation": {
            "toolchain": generators.iter().map(|g| json!({
                "tool": g.gi_tool,
                "version": g.gi_version,
            })).collect::<Vec<_>>(),
            "ir_version": &ir.ir_version,
        },
        "source": {
            "backend": &ir.ir_backend,
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
    generate_index_filtered(ir, transport, None)
}

/// Generate top-level index.ts, optionally restricting re-exports to a set of namespaces.
/// When `only_namespaces` is `Some`, only those namespaces are re-exported (type-dep stubs
/// are deliberately excluded from the barrel file).
fn generate_index_filtered(ir: &IR, transport: TransportEnv, only_namespaces: Option<&HashSet<String>>) -> String {
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

        // If filtering, only re-export requested plugins (not type-dep stubs)
        if let Some(only) = only_namespaces {
            if !only.contains(namespace.as_str()) {
                continue;
            }
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
