//! Intermediate Representation types
//!
//! These types match Synapse's IR output format exactly for deserialization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level IR structure (matches Synapse output)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IR {
    /// IR format version
    pub ir_version: String,
    /// Plexus hash for versioning (optional, computed from schema tree)
    #[serde(default)]
    pub ir_hash: Option<String>,
    /// Named type definitions (structs, enums, aliases)
    pub ir_types: HashMap<String, TypeDef>,
    /// Method definitions keyed by full path (e.g., "cone.chat")
    pub ir_methods: HashMap<String, MethodDef>,
    /// Plugin -> method names mapping
    pub ir_plugins: HashMap<String, Vec<String>>,
}

/// Type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeDef {
    pub td_name: String,
    #[serde(default)]
    pub td_description: Option<String>,
    pub td_kind: TypeKind,
}

/// Kind of type (Haskell-style tagged union)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tag")]
pub enum TypeKind {
    /// Struct with named fields
    KindStruct {
        #[serde(rename = "ksFields")]
        ks_fields: Vec<FieldDef>,
    },
    /// Tagged union (discriminated by "type" field)
    KindEnum {
        /// Field that discriminates (e.g., "type")
        #[serde(rename = "keDiscriminator")]
        ke_discriminator: String,
        #[serde(rename = "keVariants")]
        ke_variants: Vec<VariantDef>,
    },
    /// Type alias
    KindAlias {
        #[serde(rename = "kaTarget")]
        ka_target: TypeRef,
    },
    /// Primitive type
    KindPrimitive {
        #[serde(rename = "kpType")]
        kp_type: String,
        #[serde(rename = "kpFormat")]
        kp_format: Option<String>,
    },
    /// String enum (simple enum with string values)
    KindStringEnum {
        #[serde(rename = "kseValues")]
        kse_values: Vec<String>,
    },
}

/// Field in a struct
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    pub fd_name: String,
    pub fd_type: TypeRef,
    #[serde(default)]
    pub fd_description: Option<String>,
    #[serde(default)]
    pub fd_required: bool,
    #[serde(default)]
    pub fd_default: Option<serde_json::Value>,
}

/// Variant in an enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantDef {
    pub vd_name: String,
    #[serde(default)]
    pub vd_description: Option<String>,
    #[serde(default)]
    pub vd_fields: Vec<FieldDef>,
}

/// Reference to a type (Haskell-style tagged union with contents)
///
/// Haskell Aeson emits:
/// - Variants with data: {"tag": "RefNamed", "contents": "TypeName"}
/// - Unit variants: {"tag": "RefAny"} (no contents field)
///
/// We use a custom deserializer to handle both cases.
#[derive(Debug, Clone, Serialize)]
pub enum TypeRef {
    /// Named type reference
    RefNamed(String),
    /// Primitive type with optional format
    RefPrimitive(String, Option<String>),
    /// Array type
    RefArray(Box<TypeRef>),
    /// Optional type
    RefOptional(Box<TypeRef>),
    /// Intentionally dynamic (serde_json::Value) - accepts any JSON, no warning
    RefAny,
    /// Unknown type (schema gap) - should warn
    RefUnknown,
}

impl<'de> serde::Deserialize<'de> for TypeRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value.as_object().ok_or_else(|| D::Error::custom("expected object"))?;

        let tag = obj.get("tag")
            .and_then(|v| v.as_str())
            .ok_or_else(|| D::Error::custom("missing tag field"))?;

        match tag {
            "RefNamed" => {
                let contents = obj.get("contents")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| D::Error::custom("RefNamed requires string contents"))?;
                Ok(TypeRef::RefNamed(contents.to_string()))
            }
            "RefPrimitive" => {
                let contents = obj.get("contents")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| D::Error::custom("RefPrimitive requires array contents"))?;
                let prim = contents.get(0)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| D::Error::custom("RefPrimitive[0] must be string"))?;
                let format = contents.get(1)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Ok(TypeRef::RefPrimitive(prim.to_string(), format))
            }
            "RefArray" => {
                let contents = obj.get("contents")
                    .ok_or_else(|| D::Error::custom("RefArray requires contents"))?;
                let inner: TypeRef = serde_json::from_value(contents.clone())
                    .map_err(|e| D::Error::custom(format!("RefArray inner: {}", e)))?;
                Ok(TypeRef::RefArray(Box::new(inner)))
            }
            "RefOptional" => {
                let contents = obj.get("contents")
                    .ok_or_else(|| D::Error::custom("RefOptional requires contents"))?;
                let inner: TypeRef = serde_json::from_value(contents.clone())
                    .map_err(|e| D::Error::custom(format!("RefOptional inner: {}", e)))?;
                Ok(TypeRef::RefOptional(Box::new(inner)))
            }
            "RefAny" => Ok(TypeRef::RefAny),
            "RefUnknown" => Ok(TypeRef::RefUnknown),
            other => Err(D::Error::custom(format!("unknown TypeRef tag: {}", other))),
        }
    }
}

/// Method definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodDef {
    pub md_name: String,
    pub md_full_path: String,
    pub md_namespace: String,
    #[serde(default)]
    pub md_description: Option<String>,
    #[serde(default)]
    pub md_streaming: bool,
    #[serde(default)]
    pub md_params: Vec<ParamDef>,
    pub md_returns: TypeRef,
}

/// Parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamDef {
    pub pd_name: String,
    pub pd_type: TypeRef,
    #[serde(default)]
    pub pd_description: Option<String>,
    #[serde(default)]
    pub pd_required: bool,
    #[serde(default)]
    pub pd_default: Option<serde_json::Value>,
}

// === Helper methods for code generation ===

impl TypeRef {
    /// Convert to TypeScript type string
    pub fn to_ts(&self) -> String {
        match self {
            TypeRef::RefNamed(name) => to_upper_camel(name),
            TypeRef::RefPrimitive(prim, format) => primitive_to_ts(prim, format.as_deref()),
            TypeRef::RefArray(inner) => format!("{}[]", inner.to_ts()),
            TypeRef::RefOptional(inner) => format!("{} | null", inner.to_ts()),
            TypeRef::RefAny => "unknown".to_string(),     // Intentionally dynamic
            TypeRef::RefUnknown => "unknown".to_string(), // Schema gap (will warn)
        }
    }

    /// Check if this is an unknown type (schema gap that should warn)
    pub fn is_unknown(&self) -> bool {
        matches!(self, TypeRef::RefUnknown)
    }

    /// Check if this contains an unknown type anywhere
    pub fn contains_unknown(&self) -> bool {
        match self {
            TypeRef::RefUnknown => true,
            TypeRef::RefArray(inner) => inner.contains_unknown(),
            TypeRef::RefOptional(inner) => inner.contains_unknown(),
            _ => false,
        }
    }
}

fn primitive_to_ts(prim: &str, format: Option<&str>) -> String {
    match (prim, format) {
        ("string", Some("uuid")) => "string".to_string(), // UUID as string
        ("string", _) => "string".to_string(),
        ("integer", _) | ("number", _) => "number".to_string(),
        ("boolean", _) => "boolean".to_string(),
        ("array", _) => "unknown[]".to_string(),
        ("object", _) => "Record<string, unknown>".to_string(),
        _ => "unknown".to_string(),
    }
}

fn to_upper_camel(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_ref_to_ts() {
        assert_eq!(TypeRef::RefNamed("ChatEvent".to_string()).to_ts(), "ChatEvent");
        assert_eq!(TypeRef::RefPrimitive("string".to_string(), None).to_ts(), "string");
        assert_eq!(TypeRef::RefPrimitive("string".to_string(), Some("uuid".to_string())).to_ts(), "string");
        assert_eq!(TypeRef::RefPrimitive("integer".to_string(), Some("int64".to_string())).to_ts(), "number");
        assert_eq!(TypeRef::RefArray(Box::new(TypeRef::RefNamed("Node".to_string()))).to_ts(), "Node[]");
        assert_eq!(TypeRef::RefOptional(Box::new(TypeRef::RefNamed("Pos".to_string()))).to_ts(), "Pos | null");
        assert_eq!(TypeRef::RefAny.to_ts(), "unknown");     // Intentional - no warning
        assert_eq!(TypeRef::RefUnknown.to_ts(), "unknown"); // Schema gap - will warn
    }

    #[test]
    fn test_unknown_detection() {
        assert!(!TypeRef::RefAny.is_unknown());
        assert!(TypeRef::RefUnknown.is_unknown());
        assert!(!TypeRef::RefNamed("Foo".to_string()).is_unknown());

        // contains_unknown
        assert!(!TypeRef::RefAny.contains_unknown());
        assert!(TypeRef::RefUnknown.contains_unknown());
        assert!(TypeRef::RefArray(Box::new(TypeRef::RefUnknown)).contains_unknown());
        assert!(!TypeRef::RefArray(Box::new(TypeRef::RefAny)).contains_unknown());
    }
}
