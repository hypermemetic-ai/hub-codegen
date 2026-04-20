//! Client generation for Rust

use crate::ir::{IR, MethodDef, MethodRole, TypeDef, TypeRef};
use crate::deprecation::{self, DeprecationWarning};
use std::collections::{BTreeSet, HashMap};

/// Node in namespace hierarchy tree
pub struct NamespaceNode {
    /// Name of this namespace segment (e.g., "org")
    pub name: String,

    /// Full dotted path (e.g., "hyperforge.org")
    pub full_path: String,

    /// Methods defined at this exact namespace level
    pub methods: Vec<MethodDef>,

    /// Types defined at this exact namespace level
    pub types: Vec<TypeDef>,

    /// Child namespaces
    pub children: HashMap<String, NamespaceNode>,
}

impl NamespaceNode {
    fn new(name: String, full_path: String) -> Self {
        Self {
            name,
            full_path,
            methods: Vec::new(),
            types: Vec::new(),
            children: HashMap::new(),
        }
    }
}

/// Generate base client struct with WebSocket transport
pub fn generate_base_client() -> String {
    r#"//! Auto-generated Plexus client
//! Do not edit manually

use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde_json::json;
use std::pin::Pin;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::SinkExt;

/// Plexus WebSocket client
#[derive(Clone)]
pub struct PlexusClient {
    url: String,
}

impl PlexusClient {
    /// Create a new client
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    /// Call a streaming method and return a stream of PlexusStreamItems
    pub(crate) async fn call_stream(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<PlexusStreamItem>> + Send>>> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Send JSON-RPC request
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        write
            .send(Message::Text(request.to_string()))
            .await?;

        // Return stream that processes responses
        let stream = async_stream::stream! {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Parse as JSON-RPC response
                        if let Ok(rpc_response) = serde_json::from_str::<serde_json::Value>(&text) {
                            // Extract the result field which contains the PlexusStreamItem
                            if let Some(result) = rpc_response.get("result") {
                                match serde_json::from_value::<PlexusStreamItem>(result.clone()) {
                                    Ok(item) => {
                                        let is_done = matches!(item, PlexusStreamItem::Done { .. });
                                        let is_error = matches!(item, PlexusStreamItem::Error { .. });

                                        yield Ok(item);

                                        // Stop stream on done or error
                                        if is_done || is_error {
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        yield Err(anyhow!("Failed to parse PlexusStreamItem: {}", e));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        yield Err(e.into());
                        break;
                    }
                    _ => continue,
                }
            }
        };

        Ok(Box::pin(stream))
    }

    /// Call a non-streaming method and return a single result
    pub(crate) async fn call_single<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T> {
        let mut stream = self.call_stream(method, params).await?;

        // Collect all data items
        while let Some(item) = stream.next().await {
            let item = item?;
            match item {
                PlexusStreamItem::Data { content, content_type, .. } => {
                    // Try to deserialize the content
                    return serde_json::from_value(content)
                        .map_err(|e| anyhow!("Failed to deserialize {}: {}", content_type, e));
                }
                PlexusStreamItem::Error { message, code, .. } => {
                    return Err(anyhow!("Plexus error{}: {}",
                        code.map(|c| format!(" [{}]", c)).unwrap_or_default(),
                        message
                    ));
                }
                PlexusStreamItem::Done { .. } => {
                    return Err(anyhow!("Stream completed without data"));
                }
                PlexusStreamItem::Progress { .. } => {
                    // Skip progress items
                    continue;
                }
            }
        }

        Err(anyhow!("Stream ended without result"))
    }
}
"#.to_string()
}

/// Build hierarchical namespace tree from IR
pub fn build_namespace_tree(ir: &IR) -> NamespaceNode {
    let mut root = NamespaceNode::new(String::new(), String::new());

    // Insert all methods into tree
    for method in ir.ir_methods.values() {
        if method.md_namespace.is_empty() {
            continue; // Skip core plexus methods
        }
        insert_method_into_tree(&mut root, &method.md_namespace, method.clone());
    }

    // Insert all types into tree
    for typedef in ir.ir_types.values() {
        if typedef.td_namespace.is_empty() {
            continue; // Skip core types (PlexusStreamItem, etc.)
        }
        insert_type_into_tree(&mut root, &typedef.td_namespace, typedef.clone());
    }

    root
}

/// Insert method into namespace tree at correct path
fn insert_method_into_tree(node: &mut NamespaceNode, namespace: &str, method: MethodDef) {
    let path = super::parse_namespace_path(namespace);
    insert_method_at_path(node, &path, method);
}

fn insert_method_at_path(node: &mut NamespaceNode, path: &[String], method: MethodDef) {
    if path.is_empty() {
        node.methods.push(method);
    } else {
        let child_name = &path[0];
        let child = node.children.entry(child_name.clone()).or_insert_with(|| {
            let child_path = if node.full_path.is_empty() {
                child_name.clone()
            } else {
                format!("{}.{}", node.full_path, child_name)
            };
            NamespaceNode::new(child_name.clone(), child_path)
        });
        insert_method_at_path(child, &path[1..], method);
    }
}

/// Insert type into namespace tree (similar to method)
fn insert_type_into_tree(node: &mut NamespaceNode, namespace: &str, typedef: TypeDef) {
    let path = super::parse_namespace_path(namespace);
    insert_type_at_path(node, &path, typedef);
}

fn insert_type_at_path(node: &mut NamespaceNode, path: &[String], typedef: TypeDef) {
    if path.is_empty() {
        node.types.push(typedef);
    } else {
        let child_name = &path[0];
        let child = node.children.entry(child_name.clone()).or_insert_with(|| {
            let child_path = if node.full_path.is_empty() {
                child_name.clone()
            } else {
                format!("{}.{}", node.full_path, child_name)
            };
            NamespaceNode::new(child_name.clone(), child_path)
        });
        insert_type_at_path(child, &path[1..], typedef);
    }
}

/// Generate namespace modules as hierarchical directories
pub fn generate_namespace_modules(ir: &IR) -> HashMap<String, String> {
    // Legacy entry — no deprecation emission (preserves pre-IR-7 output).
    let mut discarded: Vec<DeprecationWarning> = Vec::new();
    generate_namespace_modules_with_deprecation(ir, false, &mut discarded)
}

/// IR-7: Generate namespace modules with deprecation toggle. When
/// `emit_deprecation` is true, generated method bodies are preceded by
/// `#[deprecated(...)]` attributes and `// DEPRECATED` comments where
/// applicable; one `DeprecationWarning` is pushed per surface consumed.
pub fn generate_namespace_modules_with_deprecation(
    ir: &IR,
    emit_deprecation: bool,
    warnings: &mut Vec<DeprecationWarning>,
) -> HashMap<String, String> {
    let mut files = HashMap::new();

    // Build namespace tree
    let root = build_namespace_tree(ir);

    // Recursively generate modules from tree
    generate_namespace_node(&root, ir, &mut files, emit_deprecation, warnings);

    files
}

/// Recursively generate module file for namespace node and its children
fn generate_namespace_node(
    node: &NamespaceNode,
    ir: &IR,
    files: &mut HashMap<String, String>,
    emit_deprecation: bool,
    warnings: &mut Vec<DeprecationWarning>,
) {
    // Skip root node (it has no file)
    if !node.full_path.is_empty() {
        let mut content = vec![
            format!("//! Module for {} namespace", node.full_path),
            "//! Do not edit manually".to_string(),
            "".to_string(),
            "use crate::client::PlexusClient;".to_string(),
            "use crate::types::*;".to_string(),
            "use anyhow::{anyhow, Result};".to_string(),
            "use futures::stream::{Stream, StreamExt};".to_string(),
            "use serde::{Deserialize, Serialize};".to_string(),
            "use serde_json::json;".to_string(),
            "use std::pin::Pin;".to_string(),
            "".to_string(),
        ];

        // Declare child modules if any
        if !node.children.is_empty() {
            content.push("// Child namespaces".to_string());
            let mut child_names: Vec<_> = node.children.keys().collect();
            child_names.sort();
            for child_name in child_names {
                content.push(format!("pub mod {};", child_name));
            }
            content.push("".to_string());
        }

        // Add cross-namespace imports (UPDATED for hierarchical)
        let imports = collect_cross_namespace_imports_hierarchical(node, ir);
        if !imports.is_empty() {
            for import_path in &imports {
                content.push(import_path.clone());
            }
            content.push("".to_string());
        }

        // Generate types for this namespace level
        if !node.types.is_empty() {
            content.push("// === Types ===".to_string());
            content.push("".to_string());
            let file_path = super::namespace_to_file_path(&super::parse_namespace_path(&node.full_path));
            for typedef in &node.types {
                // IR-7: emit #[deprecated] attribute above the type when applicable.
                if emit_deprecation {
                    if let Some(info) = &typedef.td_deprecation {
                        warnings.push(DeprecationWarning {
                            kind: "type".into(),
                            name: typedef.full_name(),
                            file: file_path.clone(),
                            message: info.message.clone(),
                            since: info.since.clone(),
                            removed_in: info.removed_in.clone(),
                        });
                        content.push(deprecation::format_rust(info));
                    }
                }
                content.push(super::types::generate_typedef_with_deprecation(
                    typedef,
                    emit_deprecation,
                    &file_path,
                    warnings,
                ));
                content.push("".to_string());
            }
        }

        // IR-9: Partition methods. DynamicChild methods are rendered as
        // typed-handle structs implementing DynamicChild + (optionally)
        // Listable / Searchable. Their sibling list/search methods are
        // hidden from the flat-function surface.
        let hidden_siblings: BTreeSet<String> = node
            .methods
            .iter()
            .filter_map(|m| match &m.md_role {
                MethodRole::DynamicChild { list_method, search_method } => Some(
                    [list_method.clone(), search_method.clone()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect();

        let mut dynamic_children: Vec<&MethodDef> = Vec::new();
        let mut flat_methods: Vec<&MethodDef> = Vec::new();
        for m in &node.methods {
            match &m.md_role {
                MethodRole::DynamicChild { .. } => dynamic_children.push(m),
                _ => {
                    if hidden_siblings.contains(&m.md_name) {
                        continue;
                    }
                    flat_methods.push(m);
                }
            }
        }

        // IR-9: Emit DynamicChild trait + capability traits once per namespace
        // that contains at least one dynamic child gate. Kept as a skeleton —
        // traits are declared but full trait-impls bodies are marker-only.
        if !dynamic_children.is_empty() {
            content.push("// === IR-9 typed-handle traits (skeleton) ===".to_string());
            content.push("".to_string());
            content.push(generate_dynamic_child_trait_decls());
            content.push("".to_string());
            for method in &dynamic_children {
                content.push(generate_dynamic_child_struct(method, ir, &node.full_path));
                content.push("".to_string());
            }
        }

        // Generate methods for this namespace level
        if !flat_methods.is_empty() {
            content.push("// === Methods ===".to_string());
            content.push("".to_string());
            let file_path = super::namespace_to_file_path(&super::parse_namespace_path(&node.full_path));
            for method in &flat_methods {
                // IR-7: record warning + emit #[deprecated] above the fn.
                if emit_deprecation {
                    if let Some(info) = &method.md_deprecation {
                        warnings.push(DeprecationWarning {
                            kind: "method".into(),
                            name: method.md_full_path.clone(),
                            file: file_path.clone(),
                            message: info.message.clone(),
                            since: info.since.clone(),
                            removed_in: info.removed_in.clone(),
                        });
                        content.push(deprecation::format_rust(info));
                    }
                    // Param-level deprecation — record warnings; Rust has no
                    // per-param attribute so we emit a comment above the fn.
                    for p in &method.md_params {
                        if let Some(info) = &p.pd_deprecation {
                            warnings.push(DeprecationWarning {
                                kind: "param".into(),
                                name: format!("{}({})", method.md_full_path, p.pd_name),
                                file: file_path.clone(),
                                message: info.message.clone(),
                                since: info.since.clone(),
                                removed_in: info.removed_in.clone(),
                            });
                            content.push(format!(
                                "// DEPRECATED param `{}`: {}",
                                p.pd_name,
                                deprecation::format_body(info)
                            ));
                        }
                    }
                }
                content.push(generate_method(method, ir, &node.full_path));
                content.push("".to_string());
            }
        }

        // Write module file
        let path = super::parse_namespace_path(&node.full_path);
        let file_path = super::namespace_to_file_path(&path);
        files.insert(file_path, content.join("\n"));
    }

    // Recursively generate children
    for child in node.children.values() {
        generate_namespace_node(child, ir, files, emit_deprecation, warnings);
    }
}

/// IR-9: Declare the DynamicChild / Listable / Searchable traits.
///
/// Skeleton-only: trait method bodies are provided by per-gate structs.
/// A future revision will consolidate these into a crate-level module to
/// avoid repeated declarations per namespace.
fn generate_dynamic_child_trait_decls() -> String {
    r#"/// Typed handle for a dynamic-child activation (IR-9).
///
/// Provides a trait-bound abstraction over "look up child by name". The
/// associated `Child` type is the child's generated client struct.
#[allow(async_fn_in_trait)]
pub trait DynamicChild {
    type Child;
    async fn get(&self, name: &str) -> anyhow::Result<Option<Self::Child>>;
}

/// Capability: the gate can enumerate available child names.
#[allow(async_fn_in_trait)]
pub trait Listable {
    async fn list(&self) -> anyhow::Result<Vec<String>>;
}

/// Capability: the gate can search available child names.
#[allow(async_fn_in_trait)]
pub trait Searchable {
    async fn search(&self, query: &str) -> anyhow::Result<Vec<String>>;
}"#
        .to_string()
}

/// IR-9: Emit a per-gate struct implementing DynamicChild plus the opt-in
/// Listable / Searchable capabilities per the method's IR role.
///
/// Skeleton implementation — method bodies delegate to the RPC client via
/// `call_single` / `call_stream`. The child `get` currently returns
/// `Option<serde_json::Value>` because full child-client wiring for Rust
/// is deferred to a follow-up ticket (Rust Plexus client classes are not
/// yet emitted).
fn generate_dynamic_child_struct(method: &MethodDef, _ir: &IR, namespace: &str) -> String {
    let struct_name = format!("{}Gate", to_pascal(&method.md_name));
    let (list_method, search_method) = match &method.md_role {
        MethodRole::DynamicChild { list_method, search_method } => {
            (list_method.clone(), search_method.clone())
        }
        _ => (None, None),
    };

    let mut out = String::new();
    out.push_str(&format!(
        "/// IR-9 typed handle for `{}.{}` (dynamic child gate).\n",
        namespace, method.md_name
    ));
    out.push_str("pub struct ");
    out.push_str(&struct_name);
    out.push_str("<'a> {\n    pub client: &'a PlexusClient,\n}\n\n");

    // DynamicChild impl
    out.push_str(&format!("impl<'a> DynamicChild for {}<'a> {{\n", struct_name));
    out.push_str("    type Child = serde_json::Value;\n");
    out.push_str("    async fn get(&self, name: &str) -> anyhow::Result<Option<Self::Child>> {\n");
    out.push_str(&format!(
        "        let resp: serde_json::Value = self.client.call_single(\"{}.{}\", json!({{ \"name\": name }})).await?;\n",
        namespace, method.md_name
    ));
    out.push_str("        if resp.is_null() { Ok(None) } else { Ok(Some(resp)) }\n");
    out.push_str("    }\n");
    out.push_str("}\n");

    if let Some(list_name) = list_method {
        out.push_str(&format!("\nimpl<'a> Listable for {}<'a> {{\n", struct_name));
        out.push_str("    async fn list(&self) -> anyhow::Result<Vec<String>> {\n");
        out.push_str(&format!(
            "        self.client.call_single(\"{}.{}\", serde_json::Value::Null).await\n",
            namespace, list_name
        ));
        out.push_str("    }\n");
        out.push_str("}\n");
    }

    if let Some(search_name) = search_method {
        out.push_str(&format!("\nimpl<'a> Searchable for {}<'a> {{\n", struct_name));
        out.push_str("    async fn search(&self, query: &str) -> anyhow::Result<Vec<String>> {\n");
        out.push_str(&format!(
            "        self.client.call_single(\"{}.{}\", json!({{ \"query\": query }})).await\n",
            namespace, search_name
        ));
        out.push_str("    }\n");
        out.push_str("}\n");
    }

    out
}

fn generate_method(method: &MethodDef, ir: &IR, _namespace: &str) -> String {
    let method_name = to_snake(&method.md_name);
    let return_type = type_ref_to_rust(&method.md_returns, ir);

    // Generate doc comment
    let mut doc_lines = vec![];
    if let Some(desc) = &method.md_description {
        for line in desc.lines() {
            doc_lines.push(format!("/// {}", line.trim()));
        }
    } else {
        doc_lines.push(format!("/// Call {}", method.md_full_path));
    }

    // Generate parameter serialization
    let params_json = if method.md_params.is_empty() {
        "serde_json::Value::Null".to_string()
    } else {
        let param_fields: Vec<String> = method
            .md_params
            .iter()
            .map(|p| {
                let param_name = escape_keyword(&to_snake(&p.pd_name));
                format!("\"{}\": {}", p.pd_name, param_name)
            })
            .collect();

        format!("json!({{ {} }})", param_fields.join(", "))
    };

    // Generate parameter list
    let param_list: Vec<String> = method
        .md_params
        .iter()
        .map(|p| {
            let param_name = escape_keyword(&to_snake(&p.pd_name));
            let param_type = type_ref_to_rust(&p.pd_type, ir);
            format!("{}: {}", param_name, param_type)
        })
        .collect();

    let param_str = if param_list.is_empty() {
        "client: &PlexusClient".to_string()
    } else {
        format!("client: &PlexusClient, {}", param_list.join(", "))
    };

    // Generate return type and implementation based on streaming
    if method.md_streaming {
        // Streaming method - returns filtered stream of typed data
        format!(
            r#"{doc}
pub async fn {name}({params}) -> Result<Pin<Box<dyn Stream<Item = Result<{ret_type}>> + Send>>> {{
    let stream = client.call_stream("{full_path}", {params_json}).await?;

    // Filter and transform stream items to typed data
    let typed_stream = stream.filter_map(|item| async move {{
        match item {{
            Ok(PlexusStreamItem::Data {{ content, .. }}) => {{
                match serde_json::from_value::<{ret_type}>(content) {{
                    Ok(data) => Some(Ok(data)),
                    Err(e) => Some(Err(e.into())),
                }}
            }}
            Ok(PlexusStreamItem::Error {{ message, code, .. }}) => {{
                Some(Err(anyhow!("Plexus error{{}}: {{}}",
                    code.map(|c| format!(" [{{}}]", c)).unwrap_or_default(),
                    message
                )))
            }}
            Ok(PlexusStreamItem::Progress {{ .. }}) => None, // Skip progress
            Ok(PlexusStreamItem::Done {{ .. }}) => None, // Stream will end
            Err(e) => Some(Err(e)),
        }}
    }});

    Ok(Box::pin(typed_stream))
}}"#,
            doc = doc_lines.join("\n"),
            name = method_name,
            params = param_str,
            ret_type = return_type,
            full_path = method.md_full_path,
            params_json = params_json,
        )
    } else {
        // Non-streaming method - returns single value
        format!(
            r#"{doc}
pub async fn {name}({params}) -> Result<{ret_type}> {{
    client.call_single("{full_path}", {params_json}).await
}}"#,
            doc = doc_lines.join("\n"),
            name = method_name,
            params = param_str,
            ret_type = return_type,
            full_path = method.md_full_path,
            params_json = params_json,
        )
    }
}

/// Convert TypeRef to Rust type string
fn type_ref_to_rust(tr: &TypeRef, ir: &IR) -> String {
    match tr {
        TypeRef::RefNamed(qn) => {
            // Check if the type exists in the IR
            let full_name = qn.full_name();
            if ir.ir_types.contains_key(&full_name) {
                to_pascal(&qn.local_name())
            } else {
                // Type doesn't exist - use serde_json::Value as fallback
                "serde_json::Value".to_string()
            }
        }
        TypeRef::RefPrimitive(prim, format) => primitive_to_rust(prim, format.as_deref()),
        TypeRef::RefArray(inner) => format!("Vec<{}>", type_ref_to_rust(inner, ir)),
        TypeRef::RefOptional(inner) => format!("Option<{}>", type_ref_to_rust(inner, ir)),
        TypeRef::RefAny => "serde_json::Value".to_string(),
        TypeRef::RefUnknown => "serde_json::Value".to_string(),
    }
}

fn primitive_to_rust(prim: &str, format: Option<&str>) -> String {
    match (prim, format) {
        ("string", Some("uuid")) => "String".to_string(),
        ("string", _) => "String".to_string(),
        ("integer", Some("int64")) => "i64".to_string(),
        ("integer", Some("uint64")) => "u64".to_string(),
        ("integer", _) => "i64".to_string(),
        ("number", _) => "f64".to_string(),
        ("boolean", _) => "bool".to_string(),
        ("array", _) => "Vec<serde_json::Value>".to_string(),
        ("object", _) => "serde_json::Value".to_string(),
        _ => "serde_json::Value".to_string(),
    }
}

fn to_pascal(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize = true;
    for c in s.chars() {
        if c == '_' || c == '-' || c == '.' {
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

fn to_snake(s: &str) -> String {
    use heck::ToSnekCase;
    s.to_snek_case()
}

/// Escape Rust keywords by prefixing with r#
fn escape_keyword(name: &str) -> String {
    match name {
        "as" | "async" | "await" | "break" | "const" | "continue" | "crate" | "dyn" |
        "else" | "enum" | "extern" | "false" | "fn" | "for" | "if" | "impl" | "in" |
        "let" | "loop" | "match" | "mod" | "move" | "mut" | "pub" | "ref" | "return" |
        "self" | "Self" | "static" | "struct" | "super" | "trait" | "true" | "type" |
        "unsafe" | "use" | "where" | "while" | "yield" => format!("r#{}", name),
        _ => name.to_string(),
    }
}

/// Collect cross-namespace imports with hierarchical module paths
fn collect_cross_namespace_imports_hierarchical(
    node: &NamespaceNode,
    ir: &IR,
) -> Vec<String> {
    let mut imports = HashMap::new();

    // Scan methods
    for method in &node.methods {
        scan_type_ref_hierarchical(&method.md_returns, &node.full_path, ir, &mut imports);
        for param in &method.md_params {
            scan_type_ref_hierarchical(&param.pd_type, &node.full_path, ir, &mut imports);
        }
    }

    // Scan type fields
    use crate::ir::TypeKind;
    for typedef in &node.types {
        match &typedef.td_kind {
            TypeKind::KindStruct { ks_fields } => {
                for field in ks_fields {
                    scan_type_ref_hierarchical(&field.fd_type, &node.full_path, ir, &mut imports);
                }
            }
            TypeKind::KindEnum { ke_variants, .. } => {
                for variant in ke_variants {
                    for field in &variant.vd_fields {
                        scan_type_ref_hierarchical(&field.fd_type, &node.full_path, ir, &mut imports);
                    }
                }
            }
            TypeKind::KindAlias { ka_target } => {
                scan_type_ref_hierarchical(ka_target, &node.full_path, ir, &mut imports);
            }
            _ => {}
        }
    }

    // Convert to sorted import statements
    let mut import_statements = Vec::new();
    for (module_path, mut type_names) in imports {
        // Sort and deduplicate type names
        type_names.sort();
        type_names.dedup();

        // Generate import statement with all types from this module
        if type_names.len() == 1 {
            import_statements.push(format!("use crate::{}::{};", module_path, type_names[0]));
        } else {
            import_statements.push(format!("use crate::{}::{{{}}};", module_path, type_names.join(", ")));
        }
    }

    import_statements.sort();
    import_statements
}

fn scan_type_ref_hierarchical(
    tr: &TypeRef,
    current_namespace: &str,
    ir: &IR,
    imports: &mut HashMap<String, Vec<String>>, // (module_path, [type_names])
) {
    use std::collections::HashSet;

    match tr {
        TypeRef::RefNamed(qn) => {
            if let Some(typedef) = ir.ir_types.get(&qn.full_name()) {
                if !typedef.td_namespace.is_empty()
                    && typedef.td_namespace != current_namespace
                {
                    // Convert namespace to module path: "hyperforge.org" -> "hyperforge::org"
                    let module_path = typedef.td_namespace.replace('.', "::");
                    let type_name = to_pascal(&qn.local_name());

                    // Add to vector, avoiding duplicates
                    imports.entry(module_path)
                        .or_insert_with(Vec::new)
                        .push(type_name);
                }
            }
        }
        TypeRef::RefArray(inner) => {
            scan_type_ref_hierarchical(inner, current_namespace, ir, imports);
        }
        TypeRef::RefOptional(inner) => {
            scan_type_ref_hierarchical(inner, current_namespace, ir, imports);
        }
        _ => {}
    }
}

/// Collect types from other namespaces that are referenced in this namespace's methods
fn collect_cross_namespace_type_imports(ir: &IR, current_namespace: &str) -> HashMap<String, Vec<String>> {
    use crate::ir::TypeRef;
    use std::collections::HashMap;

    let mut imports: HashMap<String, Vec<String>> = HashMap::new();

    fn collect_from_type_ref(tr: &TypeRef, ir: &IR, current_namespace: &str, imports: &mut HashMap<String, Vec<String>>) {
        match tr {
            TypeRef::RefNamed(qn) => {
                // Find the type definition to get its namespace
                if let Some(typedef) = ir.ir_types.get(&qn.full_name()) {
                    if !typedef.td_namespace.is_empty() && typedef.td_namespace != current_namespace {
                        imports
                            .entry(typedef.td_namespace.clone())
                            .or_default()
                            .push(to_pascal(&qn.local_name()));
                    }
                }
            }
            TypeRef::RefArray(inner) => collect_from_type_ref(inner, ir, current_namespace, imports),
            TypeRef::RefOptional(inner) => collect_from_type_ref(inner, ir, current_namespace, imports),
            _ => {}
        }
    }

    // Collect from method parameters and return types
    for method in ir.ir_methods.values() {
        if method.md_namespace == current_namespace {
            // Check return type
            collect_from_type_ref(&method.md_returns, ir, current_namespace, &mut imports);

            // Check parameters
            for param in &method.md_params {
                collect_from_type_ref(&param.pd_type, ir, current_namespace, &mut imports);
            }
        }
    }

    // Collect from type definitions in this namespace
    use crate::ir::TypeKind;
    for typedef in ir.ir_types.values() {
        if typedef.td_namespace == current_namespace {
            match &typedef.td_kind {
                TypeKind::KindStruct { ks_fields } => {
                    for field in ks_fields {
                        collect_from_type_ref(&field.fd_type, ir, current_namespace, &mut imports);
                    }
                }
                TypeKind::KindEnum { ke_variants, .. } => {
                    for variant in ke_variants {
                        for field in &variant.vd_fields {
                            collect_from_type_ref(&field.fd_type, ir, current_namespace, &mut imports);
                        }
                    }
                }
                TypeKind::KindAlias { ka_target } => {
                    collect_from_type_ref(ka_target, ir, current_namespace, &mut imports);
                }
                _ => {}
            }
        }
    }

    // Deduplicate and sort
    for types in imports.values_mut() {
        types.sort();
        types.dedup();
    }

    imports
}
