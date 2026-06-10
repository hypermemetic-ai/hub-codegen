//! Credential-requirement surfacing (R-4).
//!
//! When a method in the IR carries any of the R-2 wire fields —
//! `requires_credential`, `auth_posture`, `public` — hub-codegen surfaces
//! the requirement as typed metadata plus doc comments on the generated
//! client. This module holds the shared helpers used by both the
//! TypeScript and Rust backends (same role `deprecation.rs` plays for IR-7).
//!
//! **Surfacing only.** Generated clients display/introspect the
//! requirement; enforcement is server-side at the gate. No client-side
//! checks are emitted.
//!
//! # Absence contract
//!
//! When no method in the IR carries any of the three fields, no
//! requirement surface is emitted at all — generated output is
//! byte-identical to pre-R-4 codegen.

use crate::ir::{MethodDef, RequiredCredential, IR};

/// True when this method carries any credential-requirement surface:
/// a populated `requires_credential`, a declared `auth_posture`, or an
/// explicit `public` flag.
pub fn method_has_surface(method: &MethodDef) -> bool {
    method.md_requires_credential.is_some()
        || method.md_auth_posture.is_some()
        || method.md_public
}

/// True when any method in the IR carries a credential-requirement surface.
pub fn ir_has_surface(ir: &IR) -> bool {
    ir.ir_methods.values().any(method_has_surface)
}

/// Render the `requires_credential` body shared by both backends:
/// `kind: <kind>, scopes: [a, b], site: <site>` with absent parts omitted.
fn format_requirement_body(req: &RequiredCredential) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(kind) = &req.kind {
        parts.push(format!("kind: {}", kind.display()));
    }
    if !req.scopes.is_empty() {
        parts.push(format!("scopes: [{}]", req.scopes.join(", ")));
    }
    if let Some(site) = &req.site_hint {
        parts.push(format!("site: {}", site.display()));
    }
    parts.join(", ")
}

// ─────────────────────────────────────────────────────────────
// TypeScript
// ─────────────────────────────────────────────────────────────

/// JSDoc breadcrumb lines for one method (TypeScript). Empty when the
/// method carries no surface. Follows the REQ-9 invented-tag convention
/// (`@requiresAuth`, `@reads-cookie`, ...).
pub fn format_ts_jsdoc(method: &MethodDef) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(req) = &method.md_requires_credential {
        lines.push(format!("@requiresCredential {}", format_requirement_body(req)));
    }
    if method.md_public {
        lines.push("@public exempt from auth — no credential required".to_string());
    }
    if let Some(posture) = &method.md_auth_posture {
        lines.push(format!("@authPosture {}", posture.as_str()));
    }
    lines
}

/// The `MethodAuthMetadata` interface emitted once per namespace client
/// file that surfaces at least one requirement.
pub fn ts_metadata_interface() -> &'static str {
    "/** Per-method credential-requirement metadata (R-4). Surfacing only — enforcement is server-side. */\n\
     export interface MethodAuthMetadata {\n\
     \x20 /** Credential the caller must hold. Absent = no scope-derived requirement. */\n\
     \x20 readonly requiresCredential?: {\n\
     \x20   /** Required credential kind (e.g. 'bearer', 'oauth_access'). Absent = any kind whose scopes match. */\n\
     \x20   readonly kind?: string;\n\
     \x20   /** Required scope set — the caller must satisfy ALL listed scopes. */\n\
     \x20   readonly scopes: readonly string[];\n\
     \x20   /** Preferred attach site (advisory), e.g. 'header:authorization'. */\n\
     \x20   readonly siteHint?: string;\n\
     \x20 };\n\
     \x20 /** Explicitly public — exempt from the default-deny gate. */\n\
     \x20 readonly public?: boolean;\n\
     \x20 /** Declared auth posture of the owning activation. */\n\
     \x20 readonly authPosture?: 'required' | 'optional' | 'mixed' | 'none';\n\
     }"
}

/// TypeScript object-literal value for one method's metadata entry, or
/// `None` when the method carries no surface. The caller supplies the
/// (camelCase) key.
pub fn format_ts_metadata_value(method: &MethodDef) -> Option<String> {
    if !method_has_surface(method) {
        return None;
    }
    let mut fields: Vec<String> = Vec::new();
    if let Some(req) = &method.md_requires_credential {
        let mut req_fields: Vec<String> = Vec::new();
        if let Some(kind) = &req.kind {
            req_fields.push(format!("kind: '{}'", kind.display()));
        }
        let scopes = req
            .scopes
            .iter()
            .map(|s| format!("'{}'", s))
            .collect::<Vec<_>>()
            .join(", ");
        req_fields.push(format!("scopes: [{}]", scopes));
        if let Some(site) = &req.site_hint {
            req_fields.push(format!("siteHint: '{}'", site.display()));
        }
        fields.push(format!("requiresCredential: {{ {} }}", req_fields.join(", ")));
    }
    if method.md_public {
        fields.push("public: true".to_string());
    }
    if let Some(posture) = &method.md_auth_posture {
        fields.push(format!("authPosture: '{}'", posture.as_str()));
    }
    Some(format!("{{ {} }}", fields.join(", ")))
}

// ─────────────────────────────────────────────────────────────
// Rust
// ─────────────────────────────────────────────────────────────

/// Type declarations appended to the generated crate's `src/client.rs`
/// when the IR surfaces at least one requirement. Namespace modules
/// reference these via `crate::client::{...}`.
pub fn rust_metadata_decls() -> &'static str {
    r#"// === Credential-requirement metadata (R-4) ===
// Surfacing only — enforcement is server-side at the gate.

/// Declared auth posture of the activation a method belongs to (R-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Credential the caller must hold to invoke a method (R-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialRequirement {
    /// Required credential kind (e.g. `"bearer"`, `"oauth_access"`,
    /// `"other:<name>"`). `None` = any kind whose scopes match.
    pub kind: Option<&'static str>,
    /// Required scope set — the caller must satisfy ALL listed scopes.
    pub scopes: &'static [&'static str],
    /// Preferred attach site (advisory), e.g. `"header:authorization"`.
    pub site_hint: Option<&'static str>,
}

/// Per-method credential-requirement metadata (R-4). Surfacing only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodAuthMetadata {
    /// Credential the caller must hold. `None` = no scope-derived requirement.
    pub requires_credential: Option<CredentialRequirement>,
    /// Explicitly public — exempt from the default-deny gate.
    pub public: bool,
    /// Declared auth posture of the owning activation.
    pub auth_posture: Option<AuthPosture>,
}"#
}

/// Doc-comment lines (without the `/// ` leader) for one method's
/// requirement surface (Rust). Empty when the method carries no surface.
pub fn format_rust_doc(method: &MethodDef) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(req) = &method.md_requires_credential {
        lines.push(format!("Requires credential — {}", format_requirement_body(req)));
    }
    if method.md_public {
        lines.push("Public — exempt from auth (no credential required)".to_string());
    }
    if let Some(posture) = &method.md_auth_posture {
        lines.push(format!("Auth posture: {}", posture.as_str()));
    }
    lines
}

/// A `pub const <NAME>: MethodAuthMetadata = ...;` item for one method,
/// or `None` when the method carries no surface. `const_name` is the
/// SHOUTY_SNAKE method name suffixed with `_AUTH` (caller-supplied).
pub fn format_rust_const(method: &MethodDef, const_name: &str) -> Option<String> {
    if !method_has_surface(method) {
        return None;
    }
    let requires = match &method.md_requires_credential {
        Some(req) => {
            let kind = match &req.kind {
                Some(k) => format!("Some(\"{}\")", k.display()),
                None => "None".to_string(),
            };
            let scopes = req
                .scopes
                .iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(", ");
            let site = match &req.site_hint {
                Some(s) => format!("Some(\"{}\")", s.display()),
                None => "None".to_string(),
            };
            format!(
                "Some(crate::client::CredentialRequirement {{ kind: {}, scopes: &[{}], site_hint: {} }})",
                kind, scopes, site
            )
        }
        None => "None".to_string(),
    };
    let posture = match &method.md_auth_posture {
        Some(p) => {
            let variant = match p {
                crate::ir::AuthPosture::Required => "Required",
                crate::ir::AuthPosture::Optional => "Optional",
                crate::ir::AuthPosture::Mixed => "Mixed",
                crate::ir::AuthPosture::None => "None",
            };
            format!("Some(crate::client::AuthPosture::{})", variant)
        }
        None => "None".to_string(),
    };
    Some(format!(
        "/// Credential-requirement metadata for `{}` (R-4). Surfacing only.\n\
         pub const {}: crate::client::MethodAuthMetadata = crate::client::MethodAuthMetadata {{\n\
         \x20   requires_credential: {},\n\
         \x20   public: {},\n\
         \x20   auth_posture: {},\n\
         }};",
        method.md_full_path, const_name, requires, method.md_public, posture
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AttachmentSite, AuthPosture, CredentialKind, MethodRole, TypeRef};

    fn method(
        requires: Option<RequiredCredential>,
        posture: Option<AuthPosture>,
        public: bool,
    ) -> MethodDef {
        MethodDef {
            md_name: "send_message".to_string(),
            md_full_path: "cone.send_message".to_string(),
            md_namespace: "cone".to_string(),
            md_description: None,
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: MethodRole::Rpc,
            md_deprecation: None,
            md_requires_credential: requires,
            md_auth_posture: posture,
            md_public: public,
        }
    }

    fn full_requirement() -> RequiredCredential {
        RequiredCredential {
            kind: Some(CredentialKind::OauthAccess),
            scopes: vec!["facet.write".to_string(), "facet.read".to_string()],
            site_hint: Some(AttachmentSite::Header {
                name: "authorization".to_string(),
            }),
        }
    }

    #[test]
    fn test_no_surface_when_fields_absent() {
        let m = method(None, None, false);
        assert!(!method_has_surface(&m));
        assert!(format_ts_jsdoc(&m).is_empty());
        assert!(format_ts_metadata_value(&m).is_none());
        assert!(format_rust_doc(&m).is_empty());
        assert!(format_rust_const(&m, "SEND_MESSAGE_AUTH").is_none());
    }

    #[test]
    fn test_ts_jsdoc_full() {
        let m = method(Some(full_requirement()), Some(AuthPosture::Required), false);
        let lines = format_ts_jsdoc(&m);
        assert_eq!(
            lines,
            vec![
                "@requiresCredential kind: oauth_access, scopes: [facet.write, facet.read], site: header:authorization",
                "@authPosture required",
            ]
        );
    }

    #[test]
    fn test_ts_jsdoc_public() {
        let m = method(None, Some(AuthPosture::Mixed), true);
        let lines = format_ts_jsdoc(&m);
        assert_eq!(
            lines,
            vec![
                "@public exempt from auth — no credential required",
                "@authPosture mixed",
            ]
        );
    }

    #[test]
    fn test_ts_metadata_value_full() {
        let m = method(Some(full_requirement()), Some(AuthPosture::Required), false);
        let v = format_ts_metadata_value(&m).unwrap();
        assert_eq!(
            v,
            "{ requiresCredential: { kind: 'oauth_access', scopes: ['facet.write', 'facet.read'], siteHint: 'header:authorization' }, authPosture: 'required' }"
        );
    }

    #[test]
    fn test_ts_metadata_value_scopes_only() {
        let req = RequiredCredential {
            kind: None,
            scopes: vec!["facet.write".to_string()],
            site_hint: None,
        };
        let m = method(Some(req), None, false);
        let v = format_ts_metadata_value(&m).unwrap();
        assert_eq!(v, "{ requiresCredential: { scopes: ['facet.write'] } }");
    }

    #[test]
    fn test_rust_doc_full() {
        let m = method(Some(full_requirement()), Some(AuthPosture::Required), false);
        let lines = format_rust_doc(&m);
        assert_eq!(
            lines,
            vec![
                "Requires credential — kind: oauth_access, scopes: [facet.write, facet.read], site: header:authorization",
                "Auth posture: required",
            ]
        );
    }

    #[test]
    fn test_rust_const_full() {
        let m = method(Some(full_requirement()), Some(AuthPosture::Required), false);
        let c = format_rust_const(&m, "SEND_MESSAGE_AUTH").unwrap();
        assert!(c.contains("pub const SEND_MESSAGE_AUTH: crate::client::MethodAuthMetadata"));
        assert!(c.contains("kind: Some(\"oauth_access\")"));
        assert!(c.contains("scopes: &[\"facet.write\", \"facet.read\"]"));
        assert!(c.contains("site_hint: Some(\"header:authorization\")"));
        assert!(c.contains("public: false"));
        assert!(c.contains("auth_posture: Some(crate::client::AuthPosture::Required)"));
    }

    #[test]
    fn test_rust_const_public_only() {
        let m = method(None, None, true);
        let c = format_rust_const(&m, "PING_AUTH").unwrap();
        assert!(c.contains("requires_credential: None"));
        assert!(c.contains("public: true"));
        assert!(c.contains("auth_posture: None"));
    }

    #[test]
    fn test_other_kind_and_site_displays() {
        let req = RequiredCredential {
            kind: Some(CredentialKind::Other {
                name: "bespoke".to_string(),
            }),
            scopes: vec![],
            site_hint: Some(AttachmentSite::FirstFrame {
                setup_method: "login".to_string(),
                param: "token".to_string(),
            }),
        };
        let m = method(Some(req), None, false);
        let lines = format_ts_jsdoc(&m);
        assert_eq!(
            lines,
            vec!["@requiresCredential kind: other:bespoke, site: first_frame:login#token"]
        );
    }
}
