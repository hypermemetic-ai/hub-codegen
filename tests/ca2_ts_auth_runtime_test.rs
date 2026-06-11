//! CA-2 integration tests (trak facet `0281174b-4459-471e-b197-ba5a770eb979`):
//! generated TS clients ACT on auth — credential provider + schema-directed
//! attachment.
//!
//! Covers the ticket's client-side surface:
//!
//! 1. Golden transport — the gauntlet-shaped fixture (fidget-spinner's
//!    `spinner.spin` gated + `spinner.status` public, post-CA-1 site_hint)
//!    produces a transport carrying the METHOD_AUTH registry, the derived
//!    CONNECTION_SITE_HINT, the credential-provider surface, the preflight,
//!    and `requires()`.
//! 2. Typed Forbidden — types.ts declares `ForbiddenError` (stream error
//!    code "-32003") parsing the unmet scope; rpc.ts helpers throw through
//!    `errorFromStreamItem`.
//! 3. Absence — an IR with no credential surface renders an empty registry
//!    and an undefined connection hint; public-only/providerless usage needs
//!    no credential machinery.
//!
//! Server-side enforcement stays server-side; what CA-2 adds client-side is
//! attachment (schema-directed), preflight (fail fast, escape-hatched), and
//! typed errors.

use hub_codegen::generator::{GenerationOptions, TransportEnv};
use hub_codegen::ir::*;
use hub_codegen::{generate_typescript, IR};
use std::collections::HashMap;
use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────
// Golden-snapshot harness (same shape as the R-4 suite)
// ─────────────────────────────────────────────────────────────

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("ca2")
}

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

// ─────────────────────────────────────────────────────────────
// Fixtures
// ─────────────────────────────────────────────────────────────

fn method(
    ns: &str,
    name: &str,
    description: &str,
    requires: Option<RequiredCredential>,
    public: bool,
) -> MethodDef {
    MethodDef {
        md_name: name.to_string(),
        md_full_path: format!("{}.{}", ns, name),
        md_namespace: ns.to_string(),
        md_description: Some(description.to_string()),
        md_streaming: false,
        md_params: vec![],
        md_returns: TypeRef::RefPrimitive("string".to_string(), None),
        md_bidir_type: None,
        md_role: Default::default(),
        md_deprecation: None,
        md_requires_credential: requires,
        md_auth_posture: None,
        md_public: public,
    }
}

fn ir_with(backend: &str, methods: Vec<MethodDef>) -> IR {
    let mut ir_methods = HashMap::new();
    let mut ir_plugins: HashMap<String, Vec<String>> = HashMap::new();
    for m in methods {
        ir_plugins
            .entry(m.md_namespace.clone())
            .or_default()
            .push(m.md_name.clone());
        ir_methods.insert(m.md_full_path.clone(), m);
    }
    IR {
        ir_version: "2.0".to_string(),
        ir_backend: backend.to_string(),
        ir_hash: Some("ca2-fixture".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods,
        ir_plugins,
        ir_plugin_deprecations: HashMap::new(),
        ir_plugin_requests: HashMap::new(),
    }
}

/// The gauntlet shape (mirrors fidget-spinner --test-auth post-CA-1):
/// `spinner.spin` gated by scope `spinner.spin` with the derived
/// `header:authorization` hint; `spinner.status` explicitly public.
fn spinner_fixture() -> IR {
    ir_with(
        "fidget-spinner",
        vec![
            method(
                "spinner",
                "spin",
                "Spin the fidget (requires scope spinner.spin)",
                Some(RequiredCredential {
                    kind: None,
                    scopes: vec!["spinner.spin".to_string()],
                    site_hint: Some(AttachmentSite::Header {
                        name: "authorization".to_string(),
                    }),
                }),
                false,
            ),
            method("spinner", "status", "Public liveness probe", None, true),
        ],
    )
}

/// Surface-free IR (pre-R-shaped backend).
fn bare_fixture() -> IR {
    ir_with(
        "bare",
        vec![method("svc", "ping", "Liveness check", None, false)],
    )
}

fn opts() -> GenerationOptions {
    GenerationOptions {
        transport: TransportEnv::Ws,
        ..GenerationOptions::default()
    }
}

// ─────────────────────────────────────────────────────────────
// 1 — Golden transport: the full CA-2 runtime surface
// ─────────────────────────────────────────────────────────────

#[test]
fn golden_transport_spinner() {
    let out = generate_typescript(&spinner_fixture(), &opts()).unwrap();
    let transport = out.files.get("transport.ts").expect("transport.ts generated");
    assert_golden("ts_transport_spinner.ts", transport);
}

#[test]
fn transport_registry_is_schema_directed() {
    let out = generate_typescript(&spinner_fixture(), &opts()).unwrap();
    let transport = out.files.get("transport.ts").unwrap();

    // Registry entries keyed by FULL path, same renderer as <Ns>MethodAuth.
    assert!(transport.contains(
        "'spinner.spin': { requiresCredential: { scopes: ['spinner.spin'], siteHint: 'header:authorization' } },"
    ));
    assert!(transport.contains("'spinner.status': { public: true },"));

    // Connection-level hint derived from the schema, not hard-coded.
    assert!(transport.contains(
        "const CONNECTION_SITE_HINT: string | undefined = 'header:authorization';"
    ));
}

#[test]
fn transport_carries_provider_preflight_and_requires() {
    let out = generate_typescript(&spinner_fixture(), &opts()).unwrap();
    let transport = out.files.get("transport.ts").unwrap();

    // Provider type: static token | async supplier | pluggable store.
    assert!(transport.contains(
        "export type Credentials = string | CredentialSupplier | CredentialStore;"
    ));
    assert!(transport.contains("credentials?: Credentials;"));

    // Preflight + typed client-side error + escape hatch.
    assert!(transport.contains("class MissingCredentialError"));
    assert!(transport.contains("preflight?: boolean;"));
    assert!(transport.contains("this.config.preflight &&"));

    // Typed requirement introspection.
    assert!(transport.contains("requires(method: string): MethodRequirements"));

    // Bearer prefixing per the server's strip_bearer (RFC 6750).
    assert!(transport.contains("`Bearer ${token}`"));

    // JSON-RPC-level Forbidden also typed.
    assert!(transport.contains("case -32003: return new ForbiddenRpcError(message, data);"));
}

// ─────────────────────────────────────────────────────────────
// 2 — Typed stream errors (the wire-real Forbidden path)
// ─────────────────────────────────────────────────────────────

#[test]
fn types_declare_typed_auth_errors() {
    let out = generate_typescript(&spinner_fixture(), &opts()).unwrap();
    let types = out.files.get("types.ts").unwrap();

    assert!(types.contains("export class ForbiddenError extends PlexusError"));
    assert!(types.contains("export class UnauthenticatedError extends PlexusError"));
    assert!(types.contains("/missing required scope '([^']+)'/"));
    assert!(types.contains("export function errorFromStreamItem"));
    assert!(types.contains("case '-32003': return new ForbiddenError"));
}

#[test]
fn rpc_helpers_throw_typed_stream_errors() {
    let out = generate_typescript(&spinner_fixture(), &opts()).unwrap();
    let rpc = out.files.get("rpc.ts").unwrap();

    assert!(rpc.contains("throw errorFromStreamItem(item);"));
    assert!(!rpc.contains("throw new PlexusError("));
    assert!(rpc.contains(
        "export { PlexusError, ForbiddenError, UnauthenticatedError } from './types';"
    ));
}

// ─────────────────────────────────────────────────────────────
// 3 — Absence: surface-free IR renders the empty registry
// ─────────────────────────────────────────────────────────────

#[test]
fn bare_ir_renders_empty_registry_and_no_hint() {
    let out = generate_typescript(&bare_fixture(), &opts()).unwrap();
    let transport = out.files.get("transport.ts").unwrap();

    assert!(transport.contains(
        "const METHOD_AUTH: { readonly [fullPath: string]: MethodAuthMetadata } = {};"
    ));
    assert!(transport.contains(
        "const CONNECTION_SITE_HINT: string | undefined = undefined;"
    ));
    // The convention fallback exists but is the documented last resort.
    assert!(transport.contains("const CONVENTION_SITE = 'cookie:access_token';"));
}

// ─────────────────────────────────────────────────────────────
// 4 — Namespace clients are untouched by CA-2 (no golden churn)
// ─────────────────────────────────────────────────────────────

#[test]
fn namespace_clients_unchanged_by_ca2() {
    let out = generate_typescript(&spinner_fixture(), &opts()).unwrap();
    let client = out.files.get("spinner/client.ts").unwrap();
    // The per-namespace MethodAuth const (R-4) is still emitted as before —
    // CA-2's runtime registry lives in the transport, not here.
    assert!(client.contains("export const SpinnerMethodAuth"));
    assert!(!client.contains("METHOD_AUTH"));
    assert!(!client.contains("CredentialStore"));
}
