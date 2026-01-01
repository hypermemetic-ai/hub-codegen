//! Namespace/plugin interface generation
//!
//! Generates Layer 2: typed client interfaces and implementations that
//! unwrap PlexusStreamItem and return domain types.

use crate::ir::{IR, MethodDef, ParamDef};
use std::collections::HashMap;

/// Generate TypeScript namespace interfaces and implementations
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
        files.insert(format!("{}.ts", namespace), content);
    }

    files
}

fn generate_namespace(namespace: &str, methods: &[&MethodDef], ir: &IR) -> String {
    let interface_name = to_pascal(namespace);

    // Collect all type imports needed
    let mut type_imports = collect_type_imports(methods, ir);
    type_imports.sort();
    type_imports.dedup();

    let type_import_str = if type_imports.is_empty() {
        String::new()
    } else {
        format!("import type {{ {} }} from './types';", type_imports.join(", "))
    };

    let mut lines = vec![
        "// Auto-generated typed client (Layer 2)".to_string(),
        "// Wraps RPC layer and unwraps PlexusStreamItem to domain types".to_string(),
        "".to_string(),
        "import type { RpcClient } from './rpc';".to_string(),
        "import { extractData, collectOne } from './rpc';".to_string(),
    ];

    if !type_import_str.is_empty() {
        lines.push(type_import_str);
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
        let params = generate_params(&method.md_params);
        let return_type = method.md_returns.to_ts();

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
        let params_signature = generate_params(&method.md_params);
        let return_type = method.md_returns.to_ts();

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
fn collect_type_imports(methods: &[&MethodDef], _ir: &IR) -> Vec<String> {
    use crate::ir::TypeRef;

    let mut imports = Vec::new();

    fn collect_from_type_ref(tr: &TypeRef, imports: &mut Vec<String>) {
        match tr {
            TypeRef::RefNamed(name) => {
                imports.push(to_pascal(name));
            }
            TypeRef::RefArray(inner) => collect_from_type_ref(inner, imports),
            TypeRef::RefOptional(inner) => collect_from_type_ref(inner, imports),
            _ => {}
        }
    }

    for method in methods {
        collect_from_type_ref(&method.md_returns, &mut imports);
        for param in &method.md_params {
            collect_from_type_ref(&param.pd_type, &mut imports);
        }
    }

    imports
}

fn generate_params(params: &[ParamDef]) -> String {
    params
        .iter()
        .map(|p| {
            let optional = if p.pd_required { "" } else { "?" };
            let ts_type = p.pd_type.to_ts();
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
