//! R-4 integration tests: typed credential requirements on generated clients.
//!
//! Covers the ticket's verification contract:
//!
//! 1. IR decode — a full IR JSON carrying the R-2 wire fields
//!    (`requires_credential`, `auth_posture`, `public`, verbatim snake_case
//!    keys per plexus-core commit 80eaba7) decodes into `MethodDef`.
//! 2. Old-IR fixture — a pre-R IR JSON with none of the fields still parses.
//! 3. Golden TS — generated client for a fixture method carrying
//!    scope + posture (+ a sibling `public` method) surfaces the JSDoc
//!    breadcrumbs and the typed `<Ns>MethodAuth` constant.
//! 4. Golden Rust — same fixture surfaces doc comments, the
//!    `<METHOD>_AUTH` const, and the metadata type decls in client.rs.
//! 5. Absence — an IR with no requirement fields emits NO requirement
//!    surface in either target (byte-identical pre-R-4 output).
//!
//! Surfacing only: nothing in this file asserts client-side enforcement,
//! because there is none — the gate enforces server-side.

use hub_codegen::generator::{GenerationOptions, TransportEnv};
use hub_codegen::ir::*;
use hub_codegen::{generate_typescript, IR};
use std::collections::HashMap;
use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────
// Golden-snapshot harness
// ─────────────────────────────────────────────────────────────

/// Directory holding the committed golden files for this suite.
fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("r4")
}

/// Compare `actual` against the committed golden file `name`, or rewrite
/// the golden when `UPDATE_GOLDEN=1` is set in the environment.
///
/// Golden inputs are chosen to be byte-deterministic: the TypeScript
/// generator sorts methods by name; for Rust (which preserves HashMap
/// iteration order per namespace) the golden fixtures put exactly one
/// method in each namespace.
fn assert_golden(name: &str, actual: &str) {
    let path = golden_dir().join(name);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(golden_dir()).expect("create golden dir");
        std::fs::write(&path, actual).expect("write golden file");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden file {} ({}). Regenerate with UPDATE_GOLDEN=1 cargo test --features all",
            path.display(),
            e
        )
    });
    assert_eq!(
        actual,
        expected,
        "generated output diverged from golden {} — if the change is intentional, \
         regenerate with UPDATE_GOLDEN=1 cargo test --features all",
        path.display()
    );
}

/// Build a `MethodDef` with the given credential surface. One method per
/// namespace keeps Rust codegen byte-deterministic for golden comparison.
fn surfaced_method(
    namespace: &str,
    name: &str,
    description: &str,
    requires: Option<RequiredCredential>,
    posture: Option<AuthPosture>,
    public: bool,
) -> MethodDef {
    MethodDef {
        md_name: name.to_string(),
        md_full_path: format!("{}.{}", namespace, name),
        md_namespace: namespace.to_string(),
        md_description: Some(description.to_string()),
        md_streaming: false,
        md_params: vec![],
        md_returns: TypeRef::RefPrimitive("string".to_string(), None),
        md_bidir_type: None,
        md_role: Default::default(),
        md_deprecation: None,
        md_requires_credential: requires,
        md_auth_posture: posture,
        md_public: public,
    }
}

/// Golden fixture: two namespaces, one method each.
/// - `cone.send_message`: scope-derived requirement (kind + 2 scopes +
///   site hint) and posture `required` — the "scope + posture" method.
/// - `echo.ping`: explicitly `public`, posture `required` — the "public"
///   method (upstream macro construction makes `public` mutually
///   exclusive with a populated `requires_credential`).
fn golden_fixture() -> IR {
    let mut ir_methods = HashMap::new();
    ir_methods.insert(
        "cone.send_message".to_string(),
        surfaced_method(
            "cone",
            "send_message",
            "Send a message",
            Some(RequiredCredential {
                kind: Some(CredentialKind::OauthAccess),
                scopes: vec!["facet.write".to_string(), "facet.read".to_string()],
                site_hint: Some(AttachmentSite::Header {
                    name: "authorization".to_string(),
                }),
            }),
            Some(AuthPosture::Required),
            false,
        ),
    );
    ir_methods.insert(
        "echo.ping".to_string(),
        surfaced_method(
            "echo",
            "ping",
            "Liveness check",
            None,
            Some(AuthPosture::Required),
            true,
        ),
    );

    let mut ir_plugins = HashMap::new();
    ir_plugins.insert("cone".to_string(), vec!["send_message".to_string()]);
    ir_plugins.insert("echo".to_string(), vec!["ping".to_string()]);

    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("r4-golden-fixture".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods,
        ir_plugins,
        ir_plugin_deprecations: HashMap::new(),
        ir_plugin_requests: HashMap::new(),
    }
}

/// IR fixture with the R-2 fields populated:
/// - `cone.send_message`: requires oauth_access + [facet.write], header
///   site hint, posture "required".
/// - `cone.ping`: explicitly public, posture "required".
/// - `cone.list`: no surface at all (pre-R-shaped method).
fn credential_fixture() -> IR {
    let mut ir_methods = HashMap::new();

    ir_methods.insert(
        "cone.send_message".to_string(),
        MethodDef {
            md_name: "send_message".to_string(),
            md_full_path: "cone.send_message".to_string(),
            md_namespace: "cone".to_string(),
            md_description: Some("Send a message".to_string()),
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: Default::default(),
            md_deprecation: None,
            md_requires_credential: Some(RequiredCredential {
                kind: Some(CredentialKind::OauthAccess),
                scopes: vec!["facet.write".to_string()],
                site_hint: Some(AttachmentSite::Header {
                    name: "authorization".to_string(),
                }),
            }),
            md_auth_posture: Some(AuthPosture::Required),
            md_public: false,
        },
    );

    ir_methods.insert(
        "cone.ping".to_string(),
        MethodDef {
            md_name: "ping".to_string(),
            md_full_path: "cone.ping".to_string(),
            md_namespace: "cone".to_string(),
            md_description: Some("Liveness check".to_string()),
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: Default::default(),
            md_deprecation: None,
            md_requires_credential: None,
            md_auth_posture: Some(AuthPosture::Required),
            md_public: true,
        },
    );

    ir_methods.insert(
        "cone.list".to_string(),
        MethodDef {
            md_name: "list".to_string(),
            md_full_path: "cone.list".to_string(),
            md_namespace: "cone".to_string(),
            md_description: Some("List things".to_string()),
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: Default::default(),
            md_deprecation: None,
            md_requires_credential: None,
            md_auth_posture: None,
            md_public: false,
        },
    );

    let mut ir_plugins = HashMap::new();
    ir_plugins.insert(
        "cone".to_string(),
        vec!["send_message".to_string(), "ping".to_string(), "list".to_string()],
    );

    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("r4-fixture".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods,
        ir_plugins,
        ir_plugin_deprecations: HashMap::new(),
        ir_plugin_requests: HashMap::new(),
    }
}

/// Same shape with every R-2 field absent (pre-R IR).
fn bare_fixture() -> IR {
    let mut ir = credential_fixture();
    for m in ir.ir_methods.values_mut() {
        m.md_requires_credential = None;
        m.md_auth_posture = None;
        m.md_public = false;
    }
    ir
}

fn default_options() -> GenerationOptions {
    GenerationOptions {
        transport: TransportEnv::Ws,
        ..GenerationOptions::default()
    }
}

// ─────────────────────────────────────────────────────────────
// 1 + 2 — IR decode with and without the fields
// ─────────────────────────────────────────────────────────────

/// Full IR JSON (not just a MethodDef) carrying the exact R-2 wire shape
/// decodes; field names are the verbatim snake_case keys.
#[test]
fn test_full_ir_decodes_credential_fields() {
    let json = r#"{
        "irVersion": "2.0",
        "irBackend": "test",
        "irTypes": {},
        "irMethods": {
            "cone.send_message": {
                "mdName": "send_message",
                "mdFullPath": "cone.send_message",
                "mdNamespace": "cone",
                "mdStreaming": false,
                "mdParams": [],
                "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]},
                "requires_credential": {
                    "kind": {"kind": "oauth_access"},
                    "scopes": ["facet.write"],
                    "site_hint": {"site": "header", "name": "authorization"}
                },
                "auth_posture": "required"
            },
            "cone.ping": {
                "mdName": "ping",
                "mdFullPath": "cone.ping",
                "mdNamespace": "cone",
                "mdStreaming": false,
                "mdParams": [],
                "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]},
                "public": true,
                "auth_posture": "required"
            }
        },
        "irPlugins": {"cone": ["send_message", "ping"]}
    }"#;
    let ir: IR = serde_json::from_str(json).expect("R-2-carrying IR should decode");

    let send = &ir.ir_methods["cone.send_message"];
    let req = send.md_requires_credential.as_ref().expect("requirement decodes");
    assert_eq!(req.kind, Some(CredentialKind::OauthAccess));
    assert_eq!(req.scopes, vec!["facet.write".to_string()]);
    assert_eq!(
        req.site_hint,
        Some(AttachmentSite::Header { name: "authorization".to_string() })
    );
    assert_eq!(send.md_auth_posture, Some(AuthPosture::Required));
    assert!(!send.md_public);

    let ping = &ir.ir_methods["cone.ping"];
    assert!(ping.md_requires_credential.is_none());
    assert!(ping.md_public);
}

/// Old IR fixture: pre-R JSON with none of the three fields must keep
/// parsing, defaulting to None/None/false.
#[test]
fn test_old_ir_fixture_still_parses() {
    let json = r#"{
        "irVersion": "2.0",
        "irBackend": "test",
        "irTypes": {},
        "irMethods": {
            "echo.ping": {
                "mdName": "ping",
                "mdFullPath": "echo.ping",
                "mdNamespace": "echo",
                "mdStreaming": false,
                "mdParams": [],
                "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]}
            }
        },
        "irPlugins": {"echo": ["ping"]}
    }"#;
    let ir: IR = serde_json::from_str(json).expect("pre-R IR should still decode");
    let m = &ir.ir_methods["echo.ping"];
    assert!(m.md_requires_credential.is_none());
    assert!(m.md_auth_posture.is_none());
    assert!(!m.md_public);
}

// ─────────────────────────────────────────────────────────────
// 3 — Golden TypeScript surface
// ─────────────────────────────────────────────────────────────

#[test]
fn test_ts_surfaces_requirement_jsdoc_and_metadata() {
    let ir = credential_fixture();
    let result = generate_typescript(&ir, &default_options()).unwrap();
    let client = result
        .files
        .get("cone/client.ts")
        .expect("cone/client.ts should be generated");

    // JSDoc breadcrumbs on the requiring method.
    assert!(
        client.contains("@requiresCredential kind: oauth_access, scopes: [facet.write], site: header:authorization"),
        "missing @requiresCredential breadcrumb:\n{}",
        client
    );
    assert!(client.contains("@authPosture required"));
    // Public method breadcrumb.
    assert!(client.contains("@public exempt from auth — no credential required"));

    // Typed metadata surface: interface + per-method constant.
    assert!(client.contains("export interface MethodAuthMetadata"));
    assert!(client.contains(
        "export const ConeMethodAuth: { readonly [method: string]: MethodAuthMetadata } = {"
    ));
    assert!(client.contains(
        "sendMessage: { requiresCredential: { kind: 'oauth_access', scopes: ['facet.write'], siteHint: 'header:authorization' }, authPosture: 'required' },"
    ));
    assert!(client.contains("ping: { public: true, authPosture: 'required' },"));

    // The surface-less method gets NO metadata entry.
    assert!(!client.contains("list: {"));

    // Surfacing only — no enforcement: the impl body is the plain RPC call.
    assert!(client.contains("const stream = this.rpc.call('cone.send_message'"));
}

/// Determinism: two consecutive generations produce identical files.
#[test]
fn test_ts_surface_is_deterministic() {
    let ir = credential_fixture();
    let a = generate_typescript(&ir, &default_options()).unwrap();
    let b = generate_typescript(&ir, &default_options()).unwrap();
    assert_eq!(a.files.get("cone/client.ts"), b.files.get("cone/client.ts"));
}

// ─────────────────────────────────────────────────────────────
// 4 — Golden Rust surface
// ─────────────────────────────────────────────────────────────

#[cfg(feature = "rust")]
#[test]
fn test_rust_surfaces_requirement_doc_and_const() {
    use hub_codegen::generate_rust;

    let ir = credential_fixture();
    let result = generate_rust(&ir).unwrap();

    // Metadata type decls land in the generated crate's client.rs.
    let client = result.files.get("src/client.rs").expect("src/client.rs generated");
    assert!(client.contains("pub struct MethodAuthMetadata"));
    assert!(client.contains("pub struct CredentialRequirement"));
    assert!(client.contains("pub enum AuthPosture"));

    // Namespace module: doc comment + typed const on the requiring method.
    let module = result.files.get("src/cone/mod.rs").expect("src/cone/mod.rs generated");
    assert!(
        module.contains("/// Requires credential — kind: oauth_access, scopes: [facet.write], site: header:authorization"),
        "missing requirement doc line:\n{}",
        module
    );
    assert!(module.contains("/// Auth posture: required"));
    assert!(module.contains("/// Public — exempt from auth (no credential required)"));

    assert!(module.contains("pub const SEND_MESSAGE_AUTH: crate::client::MethodAuthMetadata"));
    assert!(module.contains("kind: Some(\"oauth_access\")"));
    assert!(module.contains("scopes: &[\"facet.write\"]"));
    assert!(module.contains("site_hint: Some(\"header:authorization\")"));
    assert!(module.contains("auth_posture: Some(crate::client::AuthPosture::Required)"));

    assert!(module.contains("pub const PING_AUTH: crate::client::MethodAuthMetadata"));
    assert!(module.contains("public: true"));

    // The surface-less method gets no const.
    assert!(!module.contains("LIST_AUTH"));

    // Surfacing only — generated fn body is the plain RPC call.
    assert!(module.contains("client.call_single(\"cone.send_message\""));
}

// ─────────────────────────────────────────────────────────────
// 3b + 4b — Golden snapshots (full-file byte comparison)
// ─────────────────────────────────────────────────────────────

/// Golden TS: the full generated `cone/client.ts` and `echo/client.ts`
/// for the scope+posture and public methods, byte-compared against the
/// committed snapshots in `tests/golden/r4/`.
#[test]
fn test_ts_golden_snapshot() {
    let ir = golden_fixture();
    let result = generate_typescript(&ir, &default_options()).unwrap();
    assert_golden(
        "ts_cone_client.ts",
        result.files.get("cone/client.ts").expect("cone/client.ts generated"),
    );
    assert_golden(
        "ts_echo_client.ts",
        result.files.get("echo/client.ts").expect("echo/client.ts generated"),
    );
}

/// Golden Rust: the generated namespace modules (scope+posture method,
/// public method) and the base `src/client.rs` carrying the appended
/// metadata type declarations, byte-compared against committed snapshots.
#[cfg(feature = "rust")]
#[test]
fn test_rust_golden_snapshot() {
    use hub_codegen::generate_rust;

    let ir = golden_fixture();
    let result = generate_rust(&ir).unwrap();
    assert_golden(
        "rust_cone_mod.rs",
        result.files.get("src/cone/mod.rs").expect("src/cone/mod.rs generated"),
    );
    assert_golden(
        "rust_echo_mod.rs",
        result.files.get("src/echo/mod.rs").expect("src/echo/mod.rs generated"),
    );
    assert_golden(
        "rust_client.rs",
        result.files.get("src/client.rs").expect("src/client.rs generated"),
    );
}

// ─────────────────────────────────────────────────────────────
// 5 — Absence emits no requirement surface
// ─────────────────────────────────────────────────────────────

#[test]
fn test_ts_absence_emits_no_surface() {
    let ir = bare_fixture();
    let result = generate_typescript(&ir, &default_options()).unwrap();
    let client = result.files.get("cone/client.ts").expect("cone/client.ts generated");

    assert!(!client.contains("@requiresCredential"));
    assert!(!client.contains("@authPosture"));
    assert!(!client.contains("@public"));
    assert!(!client.contains("MethodAuthMetadata"));
    assert!(!client.contains("MethodAuth"));
}

#[cfg(feature = "rust")]
#[test]
fn test_rust_absence_emits_no_surface() {
    use hub_codegen::generate_rust;

    let ir = bare_fixture();
    let result = generate_rust(&ir).unwrap();

    let client = result.files.get("src/client.rs").expect("src/client.rs generated");
    assert!(!client.contains("MethodAuthMetadata"));
    assert!(!client.contains("CredentialRequirement"));

    let module = result.files.get("src/cone/mod.rs").expect("src/cone/mod.rs generated");
    assert!(!module.contains("_AUTH"));
    assert!(!module.contains("Requires credential"));
    assert!(!module.contains("Auth posture"));
}

/// Cache safety: the requirement surface changes generated content — and
/// therefore file hashes — but hash computation itself stays well-formed
/// and distinct between surfaced and bare output.
#[test]
fn test_surface_changes_file_hash_but_not_shape() {
    let with = generate_typescript(&credential_fixture(), &default_options()).unwrap();
    let without = generate_typescript(&bare_fixture(), &default_options()).unwrap();

    let h_with = with.file_hashes.get("cone/client.ts").expect("hash for surfaced client");
    let h_without = without.file_hashes.get("cone/client.ts").expect("hash for bare client");
    assert_ne!(h_with, h_without, "surfaced content must hash differently");

    // Same file set either way — the surface adds content, not files.
    let mut k1: Vec<_> = with.files.keys().collect();
    let mut k2: Vec<_> = without.files.keys().collect();
    k1.sort();
    k2.sort();
    assert_eq!(k1, k2);
}
