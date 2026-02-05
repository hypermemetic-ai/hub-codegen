//! Type generation for Rust

use crate::ir::{FieldDef, IR, TypeDef, TypeKind, TypeRef, VariantDef};

/// Generate only core transport types (PlexusStreamItem, etc.)
pub fn generate_core_types(_ir: &IR) -> String {
    let mut lines = vec![
        "//! Core transport types for Plexus protocol".to_string(),
        "//! Do not edit manually".to_string(),
        "".to_string(),
        "use serde::{Deserialize, Serialize};".to_string(),
        "".to_string(),
        generate_core_transport_types(),
    ];

    lines.join("\n")
}

/// Generate types for a specific namespace
pub fn generate_types_for_namespace(ir: &IR, namespace: &str) -> String {
    // Filter types for this namespace
    let mut namespace_types: Vec<_> = ir.ir_types.values()
        .filter(|td| td.td_namespace == namespace)
        .collect();

    // Sort for deterministic output
    namespace_types.sort_by(|a, b| a.td_name.cmp(&b.td_name));

    let mut lines = vec![];

    // Generate all type definitions for this namespace
    for typedef in namespace_types {
        lines.push(generate_typedef(typedef));
        lines.push("".to_string());
    }

    lines.join("\n")
}

/// Generate core PlexusStreamItem and related types
fn generate_core_transport_types() -> String {
    r#"/// Metadata applied by the caller when wrapping activation responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetadata {
    /// Call path through the system
    pub provenance: Vec<String>,
    /// Hash of plexus configuration for cache invalidation
    pub plexus_hash: String,
    /// Unix timestamp (seconds) when the event was wrapped
    pub timestamp: i64,
}

/// Universal stream item - all activations emit this type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PlexusStreamItem {
    /// Data payload with caller-applied metadata
    Data {
        /// Metadata from calling layer
        metadata: StreamMetadata,
        /// Type identifier for deserialization
        content_type: String,
        /// The actual payload (serialized activation event)
        content: serde_json::Value,
    },
    /// Progress update during long-running operations
    Progress {
        /// Metadata from calling layer
        metadata: StreamMetadata,
        /// Human-readable progress message
        message: String,
        /// Optional completion percentage (0.0 - 100.0)
        percentage: Option<f64>,
    },
    /// Error occurred during processing
    Error {
        /// Metadata from calling layer
        metadata: StreamMetadata,
        /// Human-readable error message
        message: String,
        /// Optional error code for programmatic handling
        code: Option<String>,
        /// Whether the operation can be retried
        recoverable: bool,
    },
    /// Stream completed successfully
    Done {
        /// Metadata from calling layer
        metadata: StreamMetadata,
    },
}

/// Error type for Plexus operations
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct PlexusError {
    pub message: String,
    pub code: Option<String>,
    pub recoverable: bool,
    pub metadata: Option<StreamMetadata>,
}
"#
    .to_string()
}

/// Generate a single type definition
pub fn generate_typedef(typedef: &TypeDef) -> String {
    let doc_comment = if let Some(desc) = &typedef.td_description {
        desc.lines()
            .map(|line| format!("/// {}\n", line.trim()))
            .collect::<String>()
    } else {
        String::new()
    };

    let type_name = to_pascal(&typedef.td_name);

    match &typedef.td_kind {
        TypeKind::KindStruct { ks_fields } => {
            format!(
                "{}#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct {} {{\n{}\n}}",
                doc_comment,
                type_name,
                generate_fields(ks_fields)
            )
        }
        TypeKind::KindEnum {
            ke_discriminator,
            ke_variants,
        } => {
            format!(
                "{}#[derive(Debug, Clone, Serialize, Deserialize)]\n#[serde(tag = \"{}\", rename_all = \"lowercase\")]\npub enum {} {{\n{}\n}}",
                doc_comment,
                ke_discriminator,
                type_name,
                generate_variants(ke_variants)
            )
        }
        TypeKind::KindAlias { ka_target } => {
            format!(
                "{}pub type {} = {};",
                doc_comment,
                type_name,
                type_ref_to_rust(ka_target)
            )
        }
        TypeKind::KindPrimitive { kp_type, kp_format } => {
            let rust_type = primitive_to_rust(kp_type, kp_format.as_deref());
            format!("{}pub type {} = {};", doc_comment, type_name, rust_type)
        }
        TypeKind::KindStringEnum { kse_values } => {
            format!(
                "{}#[derive(Debug, Clone, Serialize, Deserialize)]\npub enum {} {{\n{}\n}}",
                doc_comment,
                type_name,
                generate_string_enum_variants(kse_values)
            )
        }
    }
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

/// Generate struct fields
fn generate_fields(fields: &[FieldDef]) -> String {
    fields
        .iter()
        .map(|field| {
            let doc = if let Some(desc) = &field.fd_description {
                desc.lines()
                    .map(|line| format!("    /// {}\n", line.trim()))
                    .collect::<String>()
            } else {
                String::new()
            };

            let field_name = escape_keyword(&to_snake(&field.fd_name));
            let field_type = type_ref_to_rust(&field.fd_type);

            // Add serde rename if field name differs from original
            let serde_rename = if to_snake(&field.fd_name) != field.fd_name {
                format!("    #[serde(rename = \"{}\")]\n", field.fd_name)
            } else {
                String::new()
            };

            format!("{}{}    pub {}: {},", doc, serde_rename, field_name, field_type)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate enum variants
fn generate_variants(variants: &[VariantDef]) -> String {
    variants
        .iter()
        .map(|variant| {
            let doc = if let Some(desc) = &variant.vd_description {
                desc.lines()
                    .map(|line| format!("    /// {}\n", line.trim()))
                    .collect::<String>()
            } else {
                String::new()
            };

            let variant_name = to_pascal(&variant.vd_name);

            if variant.vd_fields.is_empty() {
                format!("{}    {},", doc, variant_name)
            } else {
                // Generate variant fields without 'pub' - enum fields inherit visibility
                let fields = variant
                    .vd_fields
                    .iter()
                    .map(|field| {
                        let doc = if let Some(desc) = &field.fd_description {
                            desc.lines()
                                .map(|line| format!("        /// {}\n", line.trim()))
                                .collect::<String>()
                        } else {
                            String::new()
                        };

                        let field_name = escape_keyword(&to_snake(&field.fd_name));
                        let field_type = type_ref_to_rust(&field.fd_type);

                        // Add serde rename if field name differs from original
                        let serde_rename = if to_snake(&field.fd_name) != field.fd_name {
                            format!("        #[serde(rename = \"{}\")]\n", field.fd_name)
                        } else {
                            String::new()
                        };

                        format!("{}{}{}: {},", doc, serde_rename, field_name, field_type)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                format!(
                    "{}    {} {{\n{}\n    }},",
                    doc, variant_name, fields
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate string enum variants
fn generate_string_enum_variants(values: &[String]) -> String {
    values
        .iter()
        .map(|value| {
            let variant_name = to_pascal(value);
            format!("    #[serde(rename = \"{}\")]\n    {},", value, variant_name)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert TypeRef to Rust type string
fn type_ref_to_rust(tr: &TypeRef) -> String {
    match tr {
        TypeRef::RefNamed(qn) => to_pascal(&qn.local_name()),
        TypeRef::RefPrimitive(prim, format) => primitive_to_rust(prim, format.as_deref()),
        TypeRef::RefArray(inner) => format!("Vec<{}>", type_ref_to_rust(inner)),
        TypeRef::RefOptional(inner) => format!("Option<{}>", type_ref_to_rust(inner)),
        TypeRef::RefAny => "serde_json::Value".to_string(),
        TypeRef::RefUnknown => "serde_json::Value".to_string(), // Fallback for unknown
    }
}

/// Convert primitive type to Rust
fn primitive_to_rust(prim: &str, format: Option<&str>) -> String {
    match (prim, format) {
        ("string", Some("uuid")) => "String".to_string(), // Could use uuid::Uuid with feature flag
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

/// Convert to PascalCase
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

/// Convert to snake_case
fn to_snake(s: &str) -> String {
    use heck::ToSnekCase;
    s.to_snek_case()
}
