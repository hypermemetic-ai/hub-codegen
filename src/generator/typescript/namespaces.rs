//! Namespace/plugin interface generation
//!
//! Generates Layer 2: typed client interfaces and implementations that
//! unwrap PlexusStreamItem and return domain types.

use crate::ir::{IR, MethodDef, MethodRole, ParamDef, TypeRef};
use std::collections::{BTreeSet, HashMap};

/// Generate TypeScript namespace client files (one per namespace).
///
/// `filter`: when `Some`, only namespaces that equal or are prefixed by an entry
/// are generated.  `None` generates all namespaces (original behaviour).
/// Files are placed in `<namespace>/client.ts` and `<namespace>/index.ts`.
pub fn generate_namespaces(ir: &IR, filter: Option<&[String]>) -> HashMap<String, String> {
    let mut files = HashMap::new();

    // IR-9: Pre-pass. Collect the set of namespaces that appear as the target
    // of a DynamicChild gate anywhere in the IR. Those namespaces' ClientImpl
    // must be exported so the parent can reference it at runtime from
    // `childClient:`. Namespaces not referenced as dynamic-child targets keep
    // the pre-ticket non-exported ClientImpl for byte-identical output.
    let dynamic_child_target_namespaces = collect_dynamic_child_target_namespaces(ir);

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
        // Skip empty namespace — those are core plexus methods
        if namespace.is_empty() {
            continue;
        }

        // Skip namespaces that don't match the filter
        if let Some(f) = filter {
            if !ns_matches_filter(&namespace, f) {
                continue;
            }
        }

        let export_impl = dynamic_child_target_namespaces.contains(&namespace);
        let content = generate_namespace(&namespace, &methods, ir, export_impl);
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

/// IR-9: Walk the IR and collect the set of namespaces that appear as the
/// target of any DynamicChild method. These namespaces' ClientImpl must be
/// exported.
fn collect_dynamic_child_target_namespaces(ir: &IR) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for method in ir.ir_methods.values() {
        if matches!(method.md_role, MethodRole::DynamicChild { .. }) {
            let inner = unwrap_return_type(&method.md_returns);
            if let TypeRef::RefNamed(qn) = inner {
                let full = qn.full_name();
                // Prefer the referenced type's declared namespace.
                let candidate = ir
                    .ir_types
                    .get(&full)
                    .map(|td| td.td_namespace.clone())
                    .or_else(|| qn.namespace().map(|s| s.to_string()));
                if let Some(ns) = candidate {
                    if !ns.is_empty() && ir.ir_plugins.contains_key(&ns) {
                        out.insert(ns);
                        continue;
                    }
                }
                // Fall back: match by local name against plugin keys.
                if let Some(plugin_ns) = ir.ir_plugins.keys().find(|p| {
                    p.ends_with(&format!(".{}", qn.local_name())) || p.as_str() == qn.local_name()
                }) {
                    out.insert(plugin_ns.clone());
                }
            }
        }
    }
    out
}

/// Returns true if `namespace` equals any filter entry or is a child of one.
fn ns_matches_filter(namespace: &str, filter: &[String]) -> bool {
    filter.iter().any(|f| f == namespace || namespace.starts_with(&format!("{f}.")))
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

fn generate_namespace(namespace: &str, methods: &[&MethodDef], ir: &IR, export_impl: bool) -> String {
    let interface_name = to_pascal(namespace);

    // Calculate relative path to root based on namespace depth
    let depth = namespace.matches('.').count();
    let to_root = "../".repeat(depth + 1);  // +1 because we're in client.ts

    // IR-9: Build the set of sibling methods that must be hidden from this
    // namespace's client because they're referenced by a DynamicChild method's
    // list_method / search_method. Those methods are exposed via the gate's
    // .list() / .search() — not as flat methods on the parent client.
    let hidden_sibling_names: BTreeSet<String> = methods
        .iter()
        .filter_map(|m| match &m.md_role {
            MethodRole::DynamicChild { list_method, search_method } => {
                Some([list_method.clone(), search_method.clone()].into_iter().flatten().collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect();

    // IR-9: Classify methods — DynamicChild gates emit typed handles, others
    // are flat methods (unless hidden as a sibling of a DynamicChild).
    let mut dynamic_children: Vec<&MethodDef> = Vec::new();
    let mut flat_methods: Vec<&MethodDef> = Vec::new();
    for m in methods {
        match &m.md_role {
            MethodRole::DynamicChild { .. } => dynamic_children.push(m),
            _ => {
                if hidden_sibling_names.contains(&m.md_name) {
                    continue;
                }
                flat_methods.push(m);
            }
        }
    }
    dynamic_children.sort_by(|a, b| a.md_name.cmp(&b.md_name));
    flat_methods.sort_by(|a, b| a.md_name.cmp(&b.md_name));

    // Collect all type imports needed (local types only - same namespace).
    // Only consider flat methods — DynamicChild return types are replaced
    // with DynamicChild<ChildClient> which doesn't reference local types.
    let mut type_imports = collect_type_imports(&flat_methods, namespace);
    type_imports.sort();
    type_imports.dedup();

    let type_import_str = if type_imports.is_empty() {
        String::new()
    } else {
        format!("import type {{ {} }} from './types';", type_imports.join(", "))
    };

    // Collect cross-namespace type imports (types from other namespaces)
    let cross_ns_imports = collect_cross_namespace_imports(&flat_methods, namespace);

    // Only import helpers that are actually used by this namespace's methods
    let uses_extract_data = flat_methods.iter().any(|m| m.md_streaming);
    let uses_collect_one  = flat_methods.iter().any(|m| !m.md_streaming);
    let uses_dynamic_child = !dynamic_children.is_empty();

    let mut rpc_helpers: Vec<&str> = [
        if uses_extract_data { Some("extractData") } else { None },
        if uses_collect_one  { Some("collectOne")  } else { None },
    ]
    .into_iter()
    .flatten()
    .collect();
    if uses_dynamic_child {
        rpc_helpers.push("makeDynamicChild");
    }

    // IR-9: Collect DynamicChild type imports (both type-only and value, since
    // the child client class is needed for the `childClient:` runtime config).
    let dynamic_child_imports = resolve_dynamic_child_imports(&dynamic_children, namespace, ir);

    let mut lines = vec![
        "// Auto-generated typed client (Layer 2)".to_string(),
        "// Wraps RPC layer and unwraps PlexusStreamItem to domain types".to_string(),
        "".to_string(),
        format!("import type {{ RpcClient }} from '{}rpc';", to_root),
    ];
    if !rpc_helpers.is_empty() {
        lines.push(format!("import {{ {} }} from '{}rpc';", rpc_helpers.join(", "), to_root));
    }

    // IR-9: Pull in the DynamicChild / Listable / Searchable type interfaces
    // from the generated rpc runtime.
    if uses_dynamic_child {
        lines.push(format!(
            "import type {{ DynamicChild, Listable, Searchable }} from '{}rpc';",
            to_root
        ));
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

    // IR-9: Add DynamicChild child-client imports. The interface (for typing)
    // is type-only; the impl class is a value (used as a constructor).
    for (child_ns, (interfaces, impls)) in &dynamic_child_imports {
        let child_path = child_ns.replace('.', "/");
        let interface_list = interfaces.iter().cloned().collect::<Vec<_>>().join(", ");
        let impl_list = impls.iter().cloned().collect::<Vec<_>>().join(", ");
        lines.push(format!(
            "import type {{ {} }} from '{}{}/client';",
            interface_list,
            to_root,
            child_path
        ));
        lines.push(format!(
            "import {{ {} }} from '{}{}/client';",
            impl_list,
            to_root,
            child_path
        ));
    }

    lines.push("".to_string());

    // Generate interface
    lines.push(format!("/** Typed client interface for {} plugin */", namespace));
    lines.push(format!("export interface {}Client {{", interface_name));

    // IR-9: DynamicChild properties appear as readonly typed handles
    for method in &dynamic_children {
        let method_name = to_camel(&method.md_name);
        let handle_type = dynamic_child_handle_type(method, ir);
        if let Some(desc) = &method.md_description {
            lines.push(format!("  /** {} */", desc));
        }
        lines.push(format!("  readonly {}: {};", method_name, handle_type));
    }

    for method in &flat_methods {
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
    // IR-9: Export the impl when this namespace is the target of a DynamicChild
    // gate — parents need to reference it as a constructor at runtime.
    // Namespaces not referenced as dynamic-child targets keep the pre-ticket
    // non-exported form, preserving byte-identical output for pre-IR schemas.
    let impl_prefix = if export_impl { "export " } else { "" };
    lines.push(format!("{}class {}ClientImpl implements {}Client {{", impl_prefix, interface_name, interface_name));
    lines.push("  private rpc: RpcClient;".to_string());

    // IR-9: DynamicChild gate fields must be initialized in the constructor
    // using makeDynamicChild. We declare them as readonly class members.
    for method in &dynamic_children {
        let method_name = to_camel(&method.md_name);
        let handle_type = dynamic_child_handle_type(method, ir);
        lines.push(format!("  readonly {}: {};", method_name, handle_type));
    }

    // IR-9: Use the compact one-line constructor form when there are no
    // DynamicChild gates — this preserves byte-identical pre-IR output.
    // Gates require a multi-line body to initialize their handles.
    if dynamic_children.is_empty() {
        lines.push("  constructor(rpc: RpcClient) { this.rpc = rpc; }".to_string());
    } else {
        lines.push("  constructor(rpc: RpcClient) {".to_string());
        lines.push("    this.rpc = rpc;".to_string());
        for method in &dynamic_children {
            let method_name = to_camel(&method.md_name);
            let handle_type = dynamic_child_handle_type(method, ir);
            // `childClient:` needs a runtime constructor, which is the impl class.
            let child_impl = dynamic_child_impl_name(method, ir)
                .unwrap_or_else(|| "undefined as unknown as (new (rpc: RpcClient) => unknown)".to_string());
            let (list_method_literal, search_method_literal) = match &method.md_role {
                MethodRole::DynamicChild { list_method, search_method } => (
                    list_method.as_deref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "null".to_string()),
                    search_method.as_deref().map(|s| format!("'{}'", s)).unwrap_or_else(|| "null".to_string()),
                ),
                _ => ("null".to_string(), "null".to_string()),
            };
            lines.push(format!(
                "    this.{} = makeDynamicChild<{}>(this.rpc, '{}', '{}', {{",
                method_name,
                dynamic_child_target_type(method, ir),
                namespace,
                method.md_name
            ));
            lines.push(format!("      listMethod: {},", list_method_literal));
            lines.push(format!("      searchMethod: {},", search_method_literal));
            lines.push(format!("      childClient: {},", child_impl));
            lines.push(format!("    }}) as {};", handle_type));
        }
        lines.push("  }".to_string());
    }
    lines.push("".to_string());

    for (i, method) in flat_methods.iter().enumerate() {
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
        if i < flat_methods.len() - 1 {
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

/// IR-9: Compute the declared type for a DynamicChild gate
/// (`DynamicChild<ChildClient>` optionally intersected with `Listable` / `Searchable`).
fn dynamic_child_handle_type(method: &MethodDef, ir: &IR) -> String {
    let child_type = dynamic_child_target_type(method, ir);
    let mut t = format!("DynamicChild<{}>", child_type);
    if let MethodRole::DynamicChild { list_method, search_method } = &method.md_role {
        if list_method.is_some() {
            t.push_str(" & Listable");
        }
        if search_method.is_some() {
            t.push_str(" & Searchable");
        }
    }
    t
}

/// IR-9: Resolve the TypeScript *type* name for a DynamicChild's child client.
/// The child client is the generated `XxxClient` interface for the namespace
/// named by the method's return type. Falls back to `unknown` when the child
/// can't be resolved — this occurs when synapse hasn't attached a schema for
/// the child, which is a diagnosable condition higher up the pipeline.
fn dynamic_child_target_type(method: &MethodDef, ir: &IR) -> String {
    dynamic_child_class_name(method, ir).unwrap_or_else(|| "unknown".to_string())
}

/// IR-9: Resolve the child client namespace for a DynamicChild method.
/// The child is identified by the namespace of the method's return type
/// (after unwrapping `Option`).
///
/// Returns `None` when the return type is not a `RefNamed` or the child
/// namespace can't be determined.
fn dynamic_child_namespace(method: &MethodDef, ir: &IR) -> Option<String> {
    let inner = unwrap_return_type(&method.md_returns);
    if let TypeRef::RefNamed(qn) = inner {
        let full = qn.full_name();
        let child_ns = ir
            .ir_types
            .get(&full)
            .map(|td| td.td_namespace.clone())
            .or_else(|| qn.namespace().map(|s| s.to_string()))?;

        if child_ns.is_empty() {
            return None;
        }

        if ir.ir_plugins.contains_key(&child_ns) {
            return Some(child_ns);
        }

        if let Some(plugin_ns) = ir.ir_plugins.keys().find(|ns| {
            ns.ends_with(&format!(".{}", qn.local_name())) || ns.as_str() == qn.local_name()
        }) {
            return Some(plugin_ns.clone());
        }

        Some(child_ns)
    } else {
        None
    }
}

/// IR-9: The child client *interface* name for typing
/// (e.g., `SolarBodyClient`).
fn dynamic_child_class_name(method: &MethodDef, ir: &IR) -> Option<String> {
    dynamic_child_namespace(method, ir).map(|ns| format!("{}Client", to_pascal(&ns)))
}

/// IR-9: The child client *implementation* class name used as a constructor
/// at runtime (e.g., `SolarBodyClientImpl`).
fn dynamic_child_impl_name(method: &MethodDef, ir: &IR) -> Option<String> {
    dynamic_child_namespace(method, ir).map(|ns| format!("{}ClientImpl", to_pascal(&ns)))
}

/// IR-9: Unwrap `Option<T>` to `T` for return-type resolution.
fn unwrap_return_type(tr: &TypeRef) -> &TypeRef {
    match tr {
        TypeRef::RefOptional(inner) => unwrap_return_type(inner),
        other => other,
    }
}

/// IR-9: Resolve imports for DynamicChild gates' child clients.
/// Returns map `namespace -> (interface_names_for_type_imports, impl_names_for_value_imports)`.
/// The impl name is needed at runtime for the `childClient:` constructor.
fn resolve_dynamic_child_imports(
    methods: &[&MethodDef],
    current_namespace: &str,
    ir: &IR,
) -> std::collections::BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> {
    let mut out: std::collections::BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = Default::default();
    for method in methods {
        if let Some(plugin_ns) = dynamic_child_namespace(method, ir) {
            if plugin_ns != current_namespace {
                let interface_name = format!("{}Client", to_pascal(&plugin_ns));
                let impl_name = format!("{}ClientImpl", to_pascal(&plugin_ns));
                let entry = out.entry(plugin_ns).or_default();
                entry.0.insert(interface_name);
                entry.1.insert(impl_name);
            }
        }
    }
    out
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
