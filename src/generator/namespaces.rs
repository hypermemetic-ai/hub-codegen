//! Namespace/plugin interface generation
//!
//! Generates Layer 2: typed client interfaces and implementations that
//! unwrap PlexusStreamItem and return domain types.

use crate::ir::{IR, MethodDef, ParamDef, split_qualified_name};
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
        let content = generate_namespace(&namespace, &methods, ir);
        files.insert(format!("{}/client.ts", namespace), content);

        // Generate index.ts that re-exports types and client
        let index_content = generate_namespace_index(&namespace);
        files.insert(format!("{}/index.ts", namespace), index_content);
    }

    files
}

fn generate_namespace_index(namespace: &str) -> String {
    format!(
        "// Auto-generated namespace module for {}\n\
         export * from './types';\n\
         export * from './client';\n",
        namespace
    )
}

fn generate_namespace(namespace: &str, methods: &[&MethodDef], _ir: &IR) -> String {
    let interface_name = to_pascal(namespace);

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

    let mut lines = vec![
        "// Auto-generated typed client (Layer 2)".to_string(),
        "// Wraps RPC layer and unwraps PlexusStreamItem to domain types".to_string(),
        "".to_string(),
        "import type { RpcClient } from '../rpc';".to_string(),
        "import { extractData, collectOne } from '../rpc';".to_string(),
    ];

    if !type_import_str.is_empty() {
        lines.push(type_import_str);
    }

    // Add cross-namespace imports
    for (other_ns, types) in cross_ns_imports {
        let import_line = format!(
            "import type {{ {} }} from '../{}/types';",
            types.join(", "),
            other_ns
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
    lines.push(format!("export class {}ClientImpl implements {}Client {{", interface_name, interface_name));
    lines.push("  constructor(private readonly rpc: RpcClient) {}".to_string());
    lines.push("".to_string());

    for method in &methods {
        let method_name = to_camel(&method.md_name);
        let full_path = &method.md_full_path;
        let params_signature = generate_params(&method.md_params, namespace);
        let return_type = method.md_returns.to_ts_in_namespace(namespace);

        // Build params object for RPC call
        let params_object = generate_params_object(&method.md_params);

        if method.md_streaming {
            // Streaming method - return AsyncGenerator
            if let Some(desc) = &method.md_description {
                lines.push(format!("  /** {} */", desc));
            }
            lines.push(format!("  async *{}({}): AsyncGenerator<{}> {{", method_name, params_signature, return_type));
            lines.push(format!("    const stream = this.rpc.call('{}', {});", full_path, params_object));
            lines.push(format!("    yield* extractData<{}>(stream);", return_type));
            lines.push("  }".to_string());
        } else {
            // Non-streaming method - return Promise
            if let Some(desc) = &method.md_description {
                lines.push(format!("  /** {} */", desc));
            }
            lines.push(format!("  async {}({}): Promise<{}> {{", method_name, params_signature, return_type));
            lines.push(format!("    const stream = this.rpc.call('{}', {});", full_path, params_object));
            lines.push(format!("    return collectOne<{}>(stream);", return_type));
            lines.push("  }".to_string());
        }
        lines.push("".to_string());
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
            TypeRef::RefNamed(name) => {
                // Only import if in same namespace
                let (ns, local) = split_qualified_name(name);
                if ns == Some(namespace) {
                    imports.push(to_pascal(local));
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
            TypeRef::RefNamed(name) => {
                let (ns, local) = split_qualified_name(name);
                // Only import if from a DIFFERENT namespace
                if let Some(other_ns) = ns {
                    if other_ns != current_namespace {
                        imports
                            .entry(other_ns.to_string())
                            .or_default()
                            .push(to_pascal(local));
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
fn generate_params_object(params: &[ParamDef]) -> String {
    if params.is_empty() {
        return "{}".to_string();
    }

    // Note: We don't need to sort here because we're using shorthand syntax
    // and the order in the object literal doesn't matter
    let fields: Vec<String> = params
        .iter()
        .map(|p| {
            let name = to_camel(&p.pd_name);
            // Use shorthand property syntax when name matches
            name
        })
        .collect();

    format!("{{ {} }}", fields.join(", "))
}

/// Convert to PascalCase
fn to_pascal(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for c in s.chars() {
        if c == '_' || c == '-' {
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
