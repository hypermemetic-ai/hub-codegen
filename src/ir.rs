//! Intermediate Representation types
//!
//! These types match Synapse's IR output format exactly for deserialization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generator tool version information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorInfo {
    /// Tool name (e.g., "synapse", "synapse-cc")
    pub gi_tool: String,
    /// Version string (e.g., "0.2.0.0")
    pub gi_version: String,
}

/// Generation metadata tracking the full toolchain
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationMetadata {
    /// All tools in the generation chain
    pub gm_generators: Vec<GeneratorInfo>,
    /// ISO 8601 timestamp of generation
    pub gm_timestamp: String,
    /// IR format version
    pub gm_ir_version: String,
}

/// Top-level IR structure (matches Synapse output)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IR {
    /// IR format version
    pub ir_version: String,
    /// Backend name (e.g., "substrate", "plexus")
    pub ir_backend: String,
    /// Plexus hash for versioning (optional, computed from schema tree)
    #[serde(default)]
    pub ir_hash: Option<String>,
    /// Generation toolchain metadata
    #[serde(default)]
    pub ir_metadata: Option<GenerationMetadata>,
    /// Named type definitions (structs, enums, aliases)
    pub ir_types: HashMap<String, TypeDef>,
    /// Method definitions keyed by full path (e.g., "cone.chat")
    pub ir_methods: HashMap<String, MethodDef>,
    /// Plugin -> method names mapping
    pub ir_plugins: HashMap<String, Vec<String>>,
    /// Optional per-plugin (activation) deprecation information (IR-7).
    ///
    /// When a plugin name appears here, the activation itself is deprecated;
    /// generated client classes for that plugin should carry a deprecation
    /// annotation. Pre-IR IRs omit this field entirely and deserialize with
    /// an empty map, producing byte-identical codegen output to pre-ticket.
    #[serde(default)]
    pub ir_plugin_deprecations: HashMap<String, DeprecationInfo>,
    /// Optional per-plugin PlexusRequest schema (REQ-5).
    ///
    /// When a plugin namespace appears here, the activation declared
    /// `request = MyRequest` in plexus-macros; the value is the JSON Schema
    /// of the request struct, with `x-plexus-source` extensions on each
    /// field describing where it comes from (cookie/header/query/derived).
    /// REQ-7 uses this to emit JSDoc breadcrumbs on every method whose
    /// activation has a request schema.
    ///
    /// Synapse emits this field as `null` when the backend has no
    /// `psRequest` schemas; the custom deserializer below turns both `null`
    /// and absent into an empty map.
    #[serde(default, deserialize_with = "deserialize_null_as_empty_map")]
    pub ir_plugin_requests: HashMap<String, serde_json::Value>,
}

fn deserialize_null_as_empty_map<'de, D>(de: D) -> Result<HashMap<String, serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Option::<HashMap<String, serde_json::Value>>::deserialize(de).map(|o| o.unwrap_or_default())
}

/// Deprecation metadata for an IR surface (IR-7).
///
/// Mirrors `plexus_core::schema::DeprecationInfo`. Carries:
/// - `since`: version at which deprecation began (e.g. `"0.5"`).
/// - `removed_in`: planned removal version (e.g. `"0.6"`).
/// - `message`: migration guidance for consumers.
///
/// When any `MethodDef`, `ParamDef`, `FieldDef`, `TypeDef`, or plugin entry
/// in the IR carries a `Some(DeprecationInfo)`, hub-codegen treats the IR
/// as post-IR and emits target-language deprecation annotations above the
/// generated surface. When all deprecation fields are `None` / absent, the
/// IR is treated as pre-IR and no annotations are emitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeprecationInfo {
    /// Version at which deprecation began (e.g. `"0.5"`).
    pub since: String,
    /// Planned removal version (e.g. `"0.6"`).
    pub removed_in: String,
    /// Human-readable migration guidance, emitted verbatim into annotations.
    pub message: String,
}

/// Credential requirement for a method (R-4).
///
/// Mirrors `plexus_core::schema::RequiredCredential` (R-2, commit `80eaba7`
/// on plexus-core `feature/R-2-credential-wire`). Carried verbatim through
/// the IR by synapse (R-3) under the wire key `requires_credential` on each
/// method definition.
///
/// Surfacing only — generated clients display/introspect the requirement;
/// enforcement happens server-side at the gate (R-5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequiredCredential {
    /// Specific kind a candidate credential must have, or `None` for
    /// "any kind whose scope set matches".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<CredentialKind>,
    /// Required scope set (conjunction — the caller must satisfy ALL).
    /// Scopes are plain strings on the wire (`facet.write`-style).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Preferred attach site for the client. Advisory only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_hint: Option<AttachmentSite>,
}

/// Credential kind (R-4). Mirrors `plexus_auth_core::CredentialKind` —
/// internally tagged on the wire: `{"kind": "bearer"}`,
/// `{"kind": "oauth_access"}`, `{"kind": "other", "name": "..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CredentialKind {
    /// Static or short-lived bearer token (JWT, opaque token).
    Bearer,
    /// Cookie-shaped session credential.
    Cookie,
    /// OAuth/OIDC access token.
    OauthAccess,
    /// OAuth/OIDC refresh token.
    OauthRefresh,
    /// OIDC ID token.
    OidcId,
    /// AWS STS credential set.
    AwsSts,
    /// Macaroon-style capability token.
    Macaroon,
    /// Custom kind for backends with bespoke schemes.
    Other {
        /// Opaque name supplied by the backend.
        name: String,
    },
}

impl CredentialKind {
    /// Human/codegen display form: the snake_case tag, with `other:<name>`
    /// for the escape-valve variant.
    pub fn display(&self) -> String {
        match self {
            CredentialKind::Bearer => "bearer".to_string(),
            CredentialKind::Cookie => "cookie".to_string(),
            CredentialKind::OauthAccess => "oauth_access".to_string(),
            CredentialKind::OauthRefresh => "oauth_refresh".to_string(),
            CredentialKind::OidcId => "oidc_id".to_string(),
            CredentialKind::AwsSts => "aws_sts".to_string(),
            CredentialKind::Macaroon => "macaroon".to_string(),
            CredentialKind::Other { name } => format!("other:{}", name),
        }
    }
}

/// Where a credential is attached on the wire (R-4). Mirrors
/// `plexus_auth_core::AttachmentSite` — internally tagged:
/// `{"site": "header", "name": "authorization"}` etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "site", rename_all = "snake_case")]
pub enum AttachmentSite {
    /// HTTP header.
    Header { name: String },
    /// HTTP cookie.
    Cookie { name: String },
    /// First-frame WS auth via a setup method parameter.
    FirstFrame { setup_method: String, param: String },
    /// In-RPC parameter on every credential-requiring method.
    InRpcParam { param: String },
}

impl AttachmentSite {
    /// Human/codegen display form, e.g. `header:authorization`,
    /// `cookie:plexus_session`, `first_frame:login#token`, `in_rpc_param:token`.
    pub fn display(&self) -> String {
        match self {
            AttachmentSite::Header { name } => format!("header:{}", name),
            AttachmentSite::Cookie { name } => format!("cookie:{}", name),
            AttachmentSite::FirstFrame { setup_method, param } => {
                format!("first_frame:{}#{}", setup_method, param)
            }
            AttachmentSite::InRpcParam { param } => format!("in_rpc_param:{}", param),
        }
    }
}

/// Declared auth posture of the activation a method belongs to (R-4).
///
/// Mirrors `plexus_core::schema::AuthPosture` — a snake_case string on the
/// wire: `"required" | "optional" | "mixed" | "none"`. `None` (absent field)
/// means the activation never declared a posture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthPosture {
    /// Every method is auth-gated or explicitly public.
    Required,
    /// No enforcement; auth may be asymmetric across methods.
    Optional,
    /// Asymmetric auth, explicitly acknowledged.
    Mixed,
    /// Affirmatively public activation; no method takes auth.
    None,
}

impl AuthPosture {
    /// The snake_case wire string for this posture.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthPosture::Required => "required",
            AuthPosture::Optional => "optional",
            AuthPosture::Mixed => "mixed",
            AuthPosture::None => "none",
        }
    }
}

/// `skip_serializing_if` helper: omit `false` booleans from the wire so the
/// additive `public` flag keeps pre-R JSON byte-identical (mirrors
/// `plexus_core::schema::is_false`).
fn is_false(v: &bool) -> bool {
    !*v
}

/// Type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeDef {
    pub td_name: String,
    pub td_namespace: String,
    #[serde(default)]
    pub td_description: Option<String>,
    pub td_kind: TypeKind,
    /// Optional deprecation metadata (IR-7). Populated from the upstream
    /// schema's `deprecation: Some(DeprecationInfo)` field when present.
    /// Pre-IR IRs omit this field and deserialize to `None`.
    #[serde(default)]
    pub td_deprecation: Option<DeprecationInfo>,
}

impl TypeDef {
    /// Compute the fully qualified type name
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.td_namespace, self.td_name)
    }
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
    /// Optional field-level deprecation metadata (IR-7).
    /// Populated from `ParamSchema.field_deprecations` in the upstream schema
    /// when this field's name matches a deprecated key.
    #[serde(default)]
    pub fd_deprecation: Option<DeprecationInfo>,
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

/// Qualified name for type references (namespace.localName)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QualifiedName {
    pub qn_namespace: String,
    pub qn_local_name: String,
}

impl QualifiedName {
    /// Get the full qualified name as "namespace.localName" or just "localName" if namespace is empty
    pub fn full_name(&self) -> String {
        if self.qn_namespace.is_empty() {
            self.qn_local_name.clone()
        } else {
            format!("{}.{}", self.qn_namespace, self.qn_local_name)
        }
    }

    /// Get the namespace, returning None if empty
    pub fn namespace(&self) -> Option<&str> {
        if self.qn_namespace.is_empty() {
            None
        } else {
            Some(&self.qn_namespace)
        }
    }

    /// Get the local name
    pub fn local_name(&self) -> &str {
        &self.qn_local_name
    }
}

/// Reference to a type (Haskell-style tagged union with contents)
///
/// Haskell Aeson emits:
/// - Variants with data: {"tag": "RefNamed", "contents": {...}}
/// - Unit variants: {"tag": "RefAny"} (no contents field)
///
/// We use a custom deserializer to handle both cases.
#[derive(Debug, Clone, Serialize)]
pub enum TypeRef {
    /// Named type reference
    RefNamed(QualifiedName),
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
                    .ok_or_else(|| D::Error::custom("RefNamed requires contents"))?;
                let qname: QualifiedName = serde_json::from_value(contents.clone())
                    .map_err(|e| D::Error::custom(format!("RefNamed contents must be QualifiedName: {}", e)))?;
                Ok(TypeRef::RefNamed(qname))
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
    /// Bidirectional channel type parameter T.
    ///
    /// When a method uses `BidirChannel<StandardRequest<T>, StandardResponse<T>>` or
    /// `Arc<StandardBidirChannel>` (the T=Value default), this field describes T.
    ///
    /// - `None`  → the method is not bidirectional, OR it uses the default
    ///             `T = serde_json::Value` (i.e., `StandardBidirChannel`)
    /// - `Some(TypeRef::RefAny)` → bidirectional with T=Value (explicit marker)
    /// - `Some(TypeRef::RefNamed(...))` → bidirectional with a specific T type
    ///
    /// # Schema field
    ///
    /// The synapse IR builder populates this from the `"bidirType"` field in the
    /// method schema JSON (emitted when `bidirectional: true` in `MethodSchema`
    /// with a non-Value `request_type`).  When the schema only has
    /// `bidirectional: true` but no `bidir_type` field, `None` is emitted.
    #[serde(default)]
    pub md_bidir_type: Option<TypeRef>,

    /// Method role classification (IR-9).
    ///
    /// Mirrors plexus-core's `MethodRole`. Defaults to `Rpc` for backwards
    /// compatibility with IR producers that pre-date IR-2 / IR-3, so pre-IR
    /// JSON with no `mdRole` field still deserializes cleanly and produces
    /// byte-identical codegen output.
    ///
    /// - `Rpc` — ordinary RPC method (default).
    /// - `StaticChild` — method returns a child activation by static name.
    /// - `DynamicChild { list_method, search_method }` — method gates a
    ///   dynamic child keyed by its argument. Optionally carries sibling
    ///   method names that enumerate / search the keyspace.
    ///
    /// # Schema field
    ///
    /// Populated from the `"mdRole"` field in the IR JSON. When synapse
    /// (Haskell) adds a role field to `MethodDef`, it should be emitted
    /// using the Haskell Aeson tag-encoding convention matching plexus-core's
    /// `#[serde(tag = "kind", rename_all = "snake_case")]`:
    ///
    /// ```json
    /// { "kind": "rpc" }
    /// { "kind": "static_child" }
    /// { "kind": "dynamic_child", "list_method": "names", "search_method": null }
    /// ```
    #[serde(default)]
    pub md_role: MethodRole,

    /// Optional method-level deprecation metadata (IR-7).
    ///
    /// Mirrors `plexus_core::MethodSchema.deprecation`. When `Some`, the
    /// generated client method carries a target-language deprecation
    /// annotation (TypeScript `@deprecated` JSDoc plus `// DEPRECATED`
    /// comment; Rust `#[deprecated(...)]` attribute).
    ///
    /// Absence (`None` or missing field in JSON) means "not deprecated" —
    /// pre-IR IR producers predate this field and deserialize to `None`.
    #[serde(default)]
    pub md_deprecation: Option<DeprecationInfo>,

    /// Credential requirement derived from the method's scope tagging (R-4).
    ///
    /// Mirrors `plexus_core::MethodSchema.requires_credential` (R-2);
    /// synapse (R-3) passes the field through verbatim, so the IR wire key
    /// is the upstream snake_case `requires_credential` — NOT camelCase.
    /// Pre-R IRs omit the field and deserialize to `None`.
    ///
    /// Surfacing only: generated clients expose the requirement as typed
    /// metadata + doc comments; they do NOT enforce it.
    #[serde(default, rename = "requires_credential", skip_serializing_if = "Option::is_none")]
    pub md_requires_credential: Option<RequiredCredential>,

    /// Declared auth posture of the activation this method belongs to (R-4).
    ///
    /// Wire key `auth_posture` (verbatim from `MethodSchema.auth_posture`),
    /// a snake_case string enum. `None` = posture-silent activation or
    /// pre-R IR.
    #[serde(default, rename = "auth_posture", skip_serializing_if = "Option::is_none")]
    pub md_auth_posture: Option<AuthPosture>,

    /// Whether this method is explicitly public — exempt from the
    /// default-deny gate (R-4). Wire key `public` (verbatim from
    /// `MethodSchema.public`); omitted on the wire when `false`, so pre-R
    /// IRs deserialize to `false`. Mutually exclusive with a populated
    /// `requires_credential` by upstream macro construction.
    #[serde(default, rename = "public", skip_serializing_if = "is_false")]
    pub md_public: bool,
}

/// Method role classification.
///
/// Mirrors `plexus_core::MethodRole` (IR-2 / IR-3). Used by codegen
/// backends to emit typed-handle clients for `DynamicChild` methods
/// (IR-9), static accessors for `StaticChild`, and flat functions for
/// `Rpc`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MethodRole {
    /// Ordinary RPC method (the default).
    #[default]
    Rpc,
    /// Method returns a child activation by static name.
    StaticChild,
    /// Method gates a dynamic child keyed by its argument.
    DynamicChild {
        /// Optional sibling method name that lists available keys.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        list_method: Option<String>,
        /// Optional sibling method name that searches available keys.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        search_method: Option<String>,
    },
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
    /// Optional parameter-level deprecation metadata (IR-7).
    /// Mirrors `plexus_core::ParamSchema.deprecation`.
    #[serde(default)]
    pub pd_deprecation: Option<DeprecationInfo>,
    /// REQ-6/REQ-9: `x-plexus-source` annotation on the param's JSON Schema.
    ///
    /// When the upstream schema declared where this param's value comes from,
    /// the annotation is preserved here as the raw JSON object:
    ///
    /// - `{ "from": "auth", "resolver": "..." }` — from `#[from_auth(expr)]`
    /// - `{ "from": "cookie", "key": "access_token" }` — from `#[from_cookie("...")]`
    /// - `{ "from": "header", "key": "origin" }` — from `#[from_header("...")]`
    /// - `{ "from": "query", "key": "..." }` — from `#[from_query("...")]`
    /// - `{ "from": "derived" }` — from `#[from_peer]` / `PlexusRequestField` newtypes
    /// - (`None`) — unannotated; treat as RPC-sourced
    ///
    /// REQ-9 uses this to emit per-method JSDoc breadcrumbs (`@requiresAuth`,
    /// `@reads-cookie`, `@server-derived`, etc.) in generated client code.
    #[serde(default)]
    pub pd_source: Option<serde_json::Value>,
}

// === Helper methods for code generation ===

impl TypeRef {
    /// Convert to TypeScript type string (fully qualified - joins namespace.Name as NamespaceName)
    pub fn to_ts(&self) -> String {
        match self {
            TypeRef::RefNamed(qname) => to_upper_camel(&qname.full_name()),
            TypeRef::RefPrimitive(prim, format) => primitive_to_ts(prim, format.as_deref()),
            TypeRef::RefArray(inner) => format!("{}[]", inner.to_ts()),
            TypeRef::RefOptional(inner) => format!("{} | null", inner.to_ts()),
            TypeRef::RefAny => "unknown".to_string(),     // Intentionally dynamic
            TypeRef::RefUnknown => "unknown".to_string(), // Schema gap (will warn)
        }
    }

    /// Convert to TypeScript type string within a namespace context
    /// Always uses local name - cross-namespace types are handled via imports
    pub fn to_ts_in_namespace(&self, current_namespace: &str) -> String {
        match self {
            TypeRef::RefNamed(qname) => {
                // Always use local name - imports handle cross-namespace references
                to_upper_camel(qname.local_name())
            }
            TypeRef::RefPrimitive(prim, format) => primitive_to_ts(prim, format.as_deref()),
            TypeRef::RefArray(inner) => format!("{}[]", inner.to_ts_in_namespace(current_namespace)),
            TypeRef::RefOptional(inner) => format!("{} | null", inner.to_ts_in_namespace(current_namespace)),
            TypeRef::RefAny => "unknown".to_string(),
            TypeRef::RefUnknown => "unknown".to_string(),
        }
    }

    /// Get the namespace from a RefNamed, if qualified
    pub fn get_namespace(&self) -> Option<&str> {
        match self {
            TypeRef::RefNamed(qname) => qname.namespace(),
            _ => None,
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
    // Handle namespace-qualified types like "cone.ListResult" → "ConeListResult"
    s.split('.')
        .map(|part| {
            let mut result = String::new();
            let mut capitalize = true;
            for c in part.chars() {
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
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qualified_name() {
        // Test with namespace
        let qn = QualifiedName {
            qn_namespace: "cone".to_string(),
            qn_local_name: "UUID".to_string(),
        };
        assert_eq!(qn.full_name(), "cone.UUID");
        assert_eq!(qn.namespace(), Some("cone"));
        assert_eq!(qn.local_name(), "UUID");

        // Test without namespace (empty)
        let qn_no_ns = QualifiedName {
            qn_namespace: "".to_string(),
            qn_local_name: "LocalType".to_string(),
        };
        assert_eq!(qn_no_ns.full_name(), "LocalType");
        assert_eq!(qn_no_ns.namespace(), None);
        assert_eq!(qn_no_ns.local_name(), "LocalType");
    }

    #[test]
    fn test_qualified_name_deserialization() {
        // Test deserializing v2.0 format with QualifiedName
        let json = r#"{
            "tag": "RefNamed",
            "contents": {
                "qnNamespace": "cone",
                "qnLocalName": "UUID"
            }
        }"#;
        let type_ref: TypeRef = serde_json::from_str(json).unwrap();

        if let TypeRef::RefNamed(qname) = type_ref {
            assert_eq!(qname.qn_namespace, "cone");
            assert_eq!(qname.qn_local_name, "UUID");
            assert_eq!(qname.full_name(), "cone.UUID");
        } else {
            panic!("Expected RefNamed variant");
        }
    }

    #[test]
    fn test_type_ref_to_ts() {
        let chat_event = TypeRef::RefNamed(QualifiedName {
            qn_namespace: "".to_string(),
            qn_local_name: "ChatEvent".to_string(),
        });
        assert_eq!(chat_event.to_ts(), "ChatEvent");

        assert_eq!(TypeRef::RefPrimitive("string".to_string(), None).to_ts(), "string");
        assert_eq!(TypeRef::RefPrimitive("string".to_string(), Some("uuid".to_string())).to_ts(), "string");
        assert_eq!(TypeRef::RefPrimitive("integer".to_string(), Some("int64".to_string())).to_ts(), "number");

        let node = TypeRef::RefNamed(QualifiedName {
            qn_namespace: "".to_string(),
            qn_local_name: "Node".to_string(),
        });
        assert_eq!(TypeRef::RefArray(Box::new(node)).to_ts(), "Node[]");

        let pos = TypeRef::RefNamed(QualifiedName {
            qn_namespace: "".to_string(),
            qn_local_name: "Pos".to_string(),
        });
        assert_eq!(TypeRef::RefOptional(Box::new(pos)).to_ts(), "Pos | null");

        assert_eq!(TypeRef::RefAny.to_ts(), "unknown");     // Intentional - no warning
        assert_eq!(TypeRef::RefUnknown.to_ts(), "unknown"); // Schema gap - will warn
    }

    #[test]
    fn test_unknown_detection() {
        assert!(!TypeRef::RefAny.is_unknown());
        assert!(TypeRef::RefUnknown.is_unknown());

        let foo = TypeRef::RefNamed(QualifiedName {
            qn_namespace: "".to_string(),
            qn_local_name: "Foo".to_string(),
        });
        assert!(!foo.is_unknown());

        // contains_unknown
        assert!(!TypeRef::RefAny.contains_unknown());
        assert!(TypeRef::RefUnknown.contains_unknown());
        assert!(TypeRef::RefArray(Box::new(TypeRef::RefUnknown)).contains_unknown());
        assert!(!TypeRef::RefArray(Box::new(TypeRef::RefAny)).contains_unknown());
    }

    /// Test that `md_bidir_type` is correctly deserialized from IR JSON.
    ///
    /// Verifies three cases:
    /// 1. Field absent       → `None` (legacy IR / non-bidir method)
    /// 2. `null`             → `None` (explicit null from synapse)
    /// 3. `{"tag":"RefAny"}` → `Some(TypeRef::RefAny)` (T=Value bidir)
    /// 4. `{"tag":"RefNamed",...}` → `Some(TypeRef::RefNamed(...))` (specific T)
    #[test]
    fn test_method_def_bidir_type_deserialization() {
        // 1. Field absent → None (backward compatibility)
        let json_no_field = r#"{
            "mdName": "wizard",
            "mdFullPath": "interactive.wizard",
            "mdNamespace": "interactive",
            "mdStreaming": true,
            "mdParams": [],
            "mdReturns": {"tag": "RefAny"}
        }"#;
        let method: MethodDef = serde_json::from_str(json_no_field).unwrap();
        assert!(method.md_bidir_type.is_none(), "absent field should default to None");

        // 2. Explicit null → None
        let json_null = r#"{
            "mdName": "wizard",
            "mdFullPath": "interactive.wizard",
            "mdNamespace": "interactive",
            "mdStreaming": true,
            "mdParams": [],
            "mdReturns": {"tag": "RefAny"},
            "mdBidirType": null
        }"#;
        let method: MethodDef = serde_json::from_str(json_null).unwrap();
        assert!(method.md_bidir_type.is_none(), "null should deserialize to None");

        // 3. RefAny → Some(TypeRef::RefAny)  (standard bidirectional, T=Value)
        let json_ref_any = r#"{
            "mdName": "wizard",
            "mdFullPath": "interactive.wizard",
            "mdNamespace": "interactive",
            "mdStreaming": true,
            "mdParams": [],
            "mdReturns": {"tag": "RefAny"},
            "mdBidirType": {"tag": "RefAny"}
        }"#;
        let method: MethodDef = serde_json::from_str(json_ref_any).unwrap();
        assert!(
            matches!(method.md_bidir_type, Some(TypeRef::RefAny)),
            "RefAny tag should deserialize to Some(RefAny)"
        );

        // 4. RefNamed → Some(TypeRef::RefNamed(...))  (typed bidirectional)
        let json_ref_named = r#"{
            "mdName": "wizard",
            "mdFullPath": "interactive.wizard",
            "mdNamespace": "interactive",
            "mdStreaming": true,
            "mdParams": [],
            "mdReturns": {"tag": "RefAny"},
            "mdBidirType": {
                "tag": "RefNamed",
                "contents": {
                    "qnNamespace": "interactive",
                    "qnLocalName": "WizardRequest"
                }
            }
        }"#;
        let method: MethodDef = serde_json::from_str(json_ref_named).unwrap();
        if let Some(TypeRef::RefNamed(qn)) = method.md_bidir_type {
            assert_eq!(qn.qn_namespace, "interactive");
            assert_eq!(qn.qn_local_name, "WizardRequest");
        } else {
            panic!("Expected Some(RefNamed(...)) for typed bidirectional");
        }
    }

    /// IR-9: `md_role` must default to `Rpc` when the field is absent,
    /// preserving backwards compatibility with pre-IR-2 IR producers.
    #[test]
    fn test_method_def_role_defaults_to_rpc_when_absent() {
        let json_no_role = r#"{
            "mdName": "ping",
            "mdFullPath": "echo.ping",
            "mdNamespace": "echo",
            "mdStreaming": false,
            "mdParams": [],
            "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]}
        }"#;
        let method: MethodDef = serde_json::from_str(json_no_role).unwrap();
        assert_eq!(method.md_role, MethodRole::Rpc);
    }

    /// IR-9: `MethodRole::DynamicChild` round-trips through serde with both
    /// capability-method fields present and absent.
    #[test]
    fn test_method_role_dynamic_child_deserialization() {
        // DynamicChild with both list and search hints
        let json = r#"{"kind":"dynamic_child","list_method":"body_names","search_method":"search_bodies"}"#;
        let role: MethodRole = serde_json::from_str(json).unwrap();
        assert_eq!(
            role,
            MethodRole::DynamicChild {
                list_method: Some("body_names".to_string()),
                search_method: Some("search_bodies".to_string()),
            }
        );

        // DynamicChild with neither hint (defaults to None/None)
        let json_bare = r#"{"kind":"dynamic_child"}"#;
        let role: MethodRole = serde_json::from_str(json_bare).unwrap();
        assert_eq!(
            role,
            MethodRole::DynamicChild {
                list_method: None,
                search_method: None,
            }
        );

        // Rpc variant
        let json_rpc = r#"{"kind":"rpc"}"#;
        let role: MethodRole = serde_json::from_str(json_rpc).unwrap();
        assert_eq!(role, MethodRole::Rpc);

        // StaticChild variant
        let json_static = r#"{"kind":"static_child"}"#;
        let role: MethodRole = serde_json::from_str(json_static).unwrap();
        assert_eq!(role, MethodRole::StaticChild);
    }

    /// R-4: A pre-R IR method JSON (no `requires_credential`, `auth_posture`,
    /// or `public` fields) must keep parsing, defaulting to None/None/false.
    #[test]
    fn test_method_def_credential_fields_default_when_absent() {
        let json = r#"{
            "mdName": "ping",
            "mdFullPath": "echo.ping",
            "mdNamespace": "echo",
            "mdStreaming": false,
            "mdParams": [],
            "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]}
        }"#;
        let method: MethodDef = serde_json::from_str(json).unwrap();
        assert!(method.md_requires_credential.is_none());
        assert!(method.md_auth_posture.is_none());
        assert!(!method.md_public);
    }

    /// R-4: Decode the exact R-2 wire shape (plexus-core `MethodSchema`,
    /// commit 80eaba7) carried verbatim through the IR by synapse (R-3):
    /// - `requires_credential`: object with internally-tagged `kind`
    ///   (`{"kind": "oauth_access"}`), string-array `scopes`, and
    ///   internally-tagged `site_hint` (`{"site": "header", "name": ...}`).
    /// - `auth_posture`: snake_case string enum.
    /// - `public`: bool, omitted on the wire when false.
    #[test]
    fn test_method_def_credential_fields_decode_wire_shape() {
        let json = r#"{
            "mdName": "send_message",
            "mdFullPath": "cone.send_message",
            "mdNamespace": "cone",
            "mdStreaming": false,
            "mdParams": [],
            "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]},
            "requires_credential": {
                "kind": {"kind": "oauth_access"},
                "scopes": ["facet.write", "facet.read"],
                "site_hint": {"site": "header", "name": "authorization"}
            },
            "auth_posture": "required"
        }"#;
        let method: MethodDef = serde_json::from_str(json).unwrap();
        let req = method.md_requires_credential.expect("requires_credential should decode");
        assert_eq!(req.kind, Some(CredentialKind::OauthAccess));
        assert_eq!(req.scopes, vec!["facet.write".to_string(), "facet.read".to_string()]);
        assert_eq!(
            req.site_hint,
            Some(AttachmentSite::Header { name: "authorization".to_string() })
        );
        assert_eq!(method.md_auth_posture, Some(AuthPosture::Required));
        assert!(!method.md_public);
    }

    /// R-4: `public: true` decodes; `requires_credential: null` (explicit
    /// null, R-2's "omitted on the wire when None" tolerance) decodes to None.
    #[test]
    fn test_method_def_public_and_null_requirement() {
        let json = r#"{
            "mdName": "ping",
            "mdFullPath": "echo.ping",
            "mdNamespace": "echo",
            "mdStreaming": false,
            "mdParams": [],
            "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]},
            "requires_credential": null,
            "public": true,
            "auth_posture": "mixed"
        }"#;
        let method: MethodDef = serde_json::from_str(json).unwrap();
        assert!(method.md_requires_credential.is_none());
        assert!(method.md_public);
        assert_eq!(method.md_auth_posture, Some(AuthPosture::Mixed));
    }

    /// R-4: The `other` credential kind carries its opaque name; minimal
    /// `requires_credential` (scopes only) decodes with kind/site None.
    #[test]
    fn test_required_credential_variants() {
        let other: CredentialKind =
            serde_json::from_str(r#"{"kind": "other", "name": "bespoke"}"#).unwrap();
        assert_eq!(other, CredentialKind::Other { name: "bespoke".to_string() });
        assert_eq!(other.display(), "other:bespoke");

        let minimal: RequiredCredential =
            serde_json::from_str(r#"{"scopes": ["facet.write"]}"#).unwrap();
        assert_eq!(minimal.kind, None);
        assert_eq!(minimal.scopes, vec!["facet.write".to_string()]);
        assert_eq!(minimal.site_hint, None);

        // All four AuthPosture wire strings round-trip.
        for (s, expected) in [
            ("\"required\"", AuthPosture::Required),
            ("\"optional\"", AuthPosture::Optional),
            ("\"mixed\"", AuthPosture::Mixed),
            ("\"none\"", AuthPosture::None),
        ] {
            let p: AuthPosture = serde_json::from_str(s).unwrap();
            assert_eq!(p, expected);
            assert_eq!(serde_json::to_string(&p).unwrap(), s.replace('\\', ""));
        }
    }

    /// IR-9: Full `MethodDef` with `mdRole` present deserializes correctly.
    #[test]
    fn test_method_def_with_dynamic_child_role() {
        let json = r#"{
            "mdName": "body",
            "mdFullPath": "solar.body",
            "mdNamespace": "solar",
            "mdStreaming": false,
            "mdParams": [],
            "mdReturns": {"tag": "RefAny"},
            "mdRole": {"kind": "dynamic_child", "list_method": "names", "search_method": null}
        }"#;
        let method: MethodDef = serde_json::from_str(json).unwrap();
        match method.md_role {
            MethodRole::DynamicChild { list_method, search_method } => {
                assert_eq!(list_method, Some("names".to_string()));
                assert_eq!(search_method, None);
            }
            other => panic!("Expected DynamicChild, got {:?}", other),
        }
    }
}
