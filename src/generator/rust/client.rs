//! Client generation for Rust

use crate::ir::{IR, MethodDef, TypeDef, TypeRef};
use std::collections::HashMap;

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
    let mut files = HashMap::new();

    // Build namespace tree
    let root = build_namespace_tree(ir);

    // Recursively generate modules from tree
    generate_namespace_node(&root, ir, &mut files);

    files
}

/// Recursively generate module file for namespace node and its children
fn generate_namespace_node(
    node: &NamespaceNode,
    ir: &IR,
    files: &mut HashMap<String, String>,
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
            for typedef in &node.types {
                content.push(super::types::generate_typedef(typedef));
                content.push("".to_string());
            }
        }

        // Generate methods for this namespace level
        if !node.methods.is_empty() {
            content.push("// === Methods ===".to_string());
            content.push("".to_string());
            for method in &node.methods {
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
        generate_namespace_node(child, ir, files);
    }
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
