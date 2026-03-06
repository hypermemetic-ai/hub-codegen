//! Namespace/plugin interface generation
//!
//! Generates Layer 2: typed client interfaces and implementations that
//! unwrap PlexusStreamItem and return domain types.

use crate::ir::{IR, MethodDef, ParamDef};
use std::collections::HashMap;

/// Generate TypeScript namespace client files (one per namespace)
/// Files are placed in `<namespace>/client.ts`
pub fn generate_namespaces(ir: &IR) -> HashMap<String, String> {
    let mut files = HashMap::new();

    // Group methods by namespace
    let mut methods_by_ns: HashMap<String, Vec<&MethodDef>> = HashMap::new();
    for method in ir.ir_methods.values() {
        methods_by_ns
            .entry(method.md_namespace.clone())
            .or_default()
            .push(method);
    }

    // Generate interface and implementation for each namespace
    for (namespace, methods) in methods_by_ns {
        // Skip empty namespace - those are core plexus methods
        if namespace.is_empty() {
            continue;
        }

        let content = generate_namespace(&namespace, &methods, ir);
        // Convert dotted namespace to directory path
        let path = namespace.replace('.', "/");
        files.insert(format!("{}/client.ts", path), content);

        // Check if this namespace has any types
        let has_types = ir.ir_types.values().any(|td| td.td_namespace == namespace);

        // Generate index.ts that re-exports types and client
        let index_content = generate_namespace_index(&namespace, has_types);
        files.insert(format!("{}/index.ts", path), index_content);
    }

    files
}

fn generate_namespace_index(namespace: &str, has_types: bool) -> String {
    let types_export = if has_types {
        "export * from './types';\n"
    } else {
        ""
    };

    format!(
        "// Auto-generated namespace module for {}\n\
         {}export * from './client';\n",
        namespace, types_export
    )
}

fn generate_namespace(namespace: &str, methods: &[&MethodDef], _ir: &IR) -> String {
    let interface_name = to_pascal(namespace);

    // Calculate relative path to root based on namespace depth
    let depth = namespace.matches('.').count();
    let to_root = "../".repeat(depth + 1);  // +1 because we're in client.ts

    // Collect all type imports needed (local types only - same namespace)
    let mut type_imports = collect_type_imports(methods, namespace);
    type_imports.sort();
    type_imports.dedup();

    let type_import_str = if type_imports.is_empty() {
        String::new()
    } else {
        format!("import type {{ {} }} from './types';", type_imports.join(", "))
    };

    // Collect cross-namespace type imports (types from other namespaces)
    let cross_ns_imports = collect_cross_namespace_imports(methods, namespace);

    // Only import helpers that are actually used by this namespace's methods
    let uses_extract_data = methods.iter().any(|m| m.md_streaming);
    let uses_collect_one  = methods.iter().any(|m| !m.md_streaming);
    let rpc_helpers: Vec<&str> = [
        if uses_extract_data { Some("extractData") } else { None },
        if uses_collect_one  { Some("collectOne")  } else { None },
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut lines = vec![
        "// Auto-generated typed client (Layer 2)".to_string(),
        "// Wraps RPC layer and unwraps PlexusStreamItem to domain types".to_string(),
        "".to_string(),
        format!("import type {{ RpcClient }} from '{}rpc';", to_root),
    ];
    if !rpc_helpers.is_empty() {
        lines.push(format!("import {{ {} }} from '{}rpc';", rpc_helpers.join(", "), to_root));
    }

    if !type_import_str.is_empty() {
        lines.push(type_import_str);
    }

    // Add cross-namespace imports
    for (other_ns, types) in cross_ns_imports {
        let other_path = other_ns.replace('.', "/");
        let import_line = format!(
            "import type {{ {} }} from '{}{}/types';",
            types.join(", "),
            to_root,
            other_path
        );
        lines.push(import_line);
    }

    lines.push("".to_string());

    // Sort methods for deterministic output
    let mut methods = methods.to_vec();
    methods.sort_by(|a, b| a.md_name.cmp(&b.md_name));

    // Generate interface
    lines.push(format!("/** Typed client interface for {} plugin */", namespace));
    lines.push(format!("export interface {}Client {{", interface_name));

    for method in &methods {
        let method_name = to_camel(&method.md_name);
        let params = generate_params(&method.md_params, namespace);
        let return_type = method.md_returns.to_ts_in_namespace(namespace);

        // Streaming methods return AsyncGenerator, non-streaming return Promise
        let full_return = if method.md_streaming {
            format!("AsyncGenerator<{}>", return_type)
        } else {
            format!("Promise<{}>", return_type)
        };

        if let Some(desc) = &method.md_description {
            lines.push(format!("  /** {} */", desc));
        }
        lines.push(format!("  {}({}): {};", method_name, params, full_return));
    }

    lines.push("}".to_string());
    lines.push("".to_string());

    // Generate implementation class
    lines.push(format!("/** Typed client implementation for {} plugin */", namespace));
    lines.push(format!("class {}ClientImpl implements {}Client {{", interface_name, interface_name));
    lines.push("  private rpc: RpcClient;".to_string());
    lines.push("  constructor(rpc: RpcClient) { this.rpc = rpc; }".to_string());
    lines.push("".to_string());

    for (i, method) in methods.iter().enumerate() {
        let method_name = to_camel(&method.md_name);
        let full_path = &method.md_full_path;
        let params_signature = generate_params(&method.md_params, namespace);
        let return_type = method.md_returns.to_ts_in_namespace(namespace);

        // Build params object for RPC call
        let params_object = generate_params_object(&method.md_params);

        if method.md_streaming {
            lines.push(format!("  async *{}({}): AsyncGenerator<{}> {{", method_name, params_signature, return_type));
            lines.push(format!("    const stream = this.rpc.call('{}', {});", full_path, params_object));
            lines.push(format!("    yield* extractData<{}>(stream);", return_type));
            lines.push("  }".to_string());
        } else {
            lines.push(format!("  async {}({}): Promise<{}> {{", method_name, params_signature, return_type));
            lines.push(format!("    const stream = this.rpc.call('{}', {});", full_path, params_object));
            lines.push(format!("    return collectOne<{}>(stream);", return_type));
            lines.push("  }".to_string());
        }
        if i < methods.len() - 1 {
            lines.push("".to_string());
        }
    }

    lines.push("}".to_string());
    lines.push("".to_string());

    // Generate factory function
    lines.push(format!("/** Create a typed {} client from an RPC client */", namespace));
    lines.push(format!("export function create{}Client(rpc: RpcClient): {}Client {{", interface_name, interface_name));
    lines.push(format!("  return new {}ClientImpl(rpc);", interface_name));
    lines.push("}".to_string());

    lines.join("\n")
}

/// Collect all named types referenced in method params and returns
/// Only includes types from the same namespace (local imports)
fn collect_type_imports(methods: &[&MethodDef], namespace: &str) -> Vec<String> {
    use crate::ir::TypeRef;

    let mut imports = Vec::new();

    fn collect_from_type_ref(tr: &TypeRef, imports: &mut Vec<String>, namespace: &str) {
        match tr {
            TypeRef::RefNamed(qn) => {
                // Only import if in same namespace
                if qn.namespace() == Some(namespace) {
                    imports.push(to_pascal(qn.local_name()));
                }
            }
            TypeRef::RefArray(inner) => collect_from_type_ref(inner, imports, namespace),
            TypeRef::RefOptional(inner) => collect_from_type_ref(inner, imports, namespace),
            _ => {}
        }
    }

    for method in methods {
        collect_from_type_ref(&method.md_returns, &mut imports, namespace);
        for param in &method.md_params {
            collect_from_type_ref(&param.pd_type, &mut imports, namespace);
        }
    }

    imports
}

/// Collect types from OTHER namespaces that need to be imported
/// Returns a map of namespace -> list of type names
/// Example: { "io" => ["SolarEvent", "BodyType"], "arbor" => ["TreeInfo"] }
fn collect_cross_namespace_imports(methods: &[&MethodDef], current_namespace: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    use crate::ir::TypeRef;
    use std::collections::BTreeMap;

    let mut imports: BTreeMap<String, Vec<String>> = BTreeMap::new();

    fn collect_from_type_ref(
        tr: &TypeRef,
        imports: &mut BTreeMap<String, Vec<String>>,
        current_namespace: &str,
    ) {
        match tr {
            TypeRef::RefNamed(qn) => {
                // Only import if from a DIFFERENT namespace
                if let Some(other_ns) = qn.namespace() {
                    if other_ns != current_namespace {
                        imports
                            .entry(other_ns.to_string())
                            .or_default()
                            .push(to_pascal(qn.local_name()));
                    }
                }
            }
            TypeRef::RefArray(inner) => collect_from_type_ref(inner, imports, current_namespace),
            TypeRef::RefOptional(inner) => collect_from_type_ref(inner, imports, current_namespace),
            _ => {}
        }
    }

    for method in methods {
        collect_from_type_ref(&method.md_returns, &mut imports, current_namespace);
        for param in &method.md_params {
            collect_from_type_ref(&param.pd_type, &mut imports, current_namespace);
        }
    }

    // Deduplicate and sort type names within each namespace
    for types in imports.values_mut() {
        types.sort();
        types.dedup();
    }

    imports
}

fn generate_params(params: &[ParamDef], namespace: &str) -> String {
    // Sort parameters: required first, then optional
    // This is required by TypeScript syntax
    let mut sorted_params = params.to_vec();
    sorted_params.sort_by_key(|p| (
        !p.pd_required,  // false (required) sorts before true (optional)
        p.pd_name.clone() // tie-breaker: alphabetical
    ));

    sorted_params
        .iter()
        .map(|p| {
            let optional = if p.pd_required { "" } else { "?" };
            let ts_type = p.pd_type.to_ts_in_namespace(namespace);
            format!("{}{}: {}", to_camel(&p.pd_name), optional, ts_type)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate a JavaScript object literal from params for the RPC call
/// Uses explicit property mapping to preserve snake_case on the wire
/// while keeping camelCase in TypeScript function signatures
fn generate_params_object(params: &[ParamDef]) -> String {
    if params.is_empty() {
        return "{}".to_string();
    }

    // Generate explicit mappings: { snake_case: camelCase }
    // This ensures RPC wire format uses snake_case as Plexus expects,
    // while TypeScript APIs remain idiomatic camelCase
    let fields: Vec<String> = params
        .iter()
        .map(|p| {
            let camel_name = to_camel(&p.pd_name);  // TypeScript variable (camelCase)
            let snake_name = &p.pd_name;             // RPC wire format (snake_case)

            // Use shorthand when names are identical, explicit mapping otherwise
            if camel_name == *snake_name {
                camel_name
            } else {
                format!("{}: {}", snake_name, camel_name)
            }
        })
        .collect();

    format!("{{ {} }}", fields.join(", "))
}

/// Convert to PascalCase
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

/// Convert to camelCase
fn to_camel(s: &str) -> String {
    let pascal = to_pascal(s);
    if pascal.is_empty() {
        return pascal;
    }
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => first.to_ascii_lowercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
