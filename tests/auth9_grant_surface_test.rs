//! AUTH-9: every client generated from a schema knows what each method
//! requires, and says so.
//!
//! 1. IR decode — `mdGrant`, `mdTarget`, `mdLongDescription` on a method and
//!    `irPluginRoles` on the IR decode; an IR without them (or with
//!    `irPluginRoles: null`) still decodes.
//! 2. Golden TypeScript — JSDoc with the long description and
//!    `@requires`, the per-activation `requires` map and `defaultRoles`.
//! 3. The typed refusal — types.ts declares `Forbidden` (a `ForbiddenError`)
//!    and `errorFromStreamItem` builds it from an `error_kind: "forbidden"`
//!    item; the RPC helpers throw what `errorFromStreamItem` returns.
//! 4. Golden Rust — doc comments, the `REQUIRES` table and `requires_of`,
//!    `DEFAULT_ROLES`, the `Requirement` type in client.rs; and the crate
//!    compiles.
//! 5. Absence — the same IR with the fields stripped generates exactly what
//!    it generated before AUTH-9 (no surface at all).

use hub_codegen::generator::{GenerationOptions, TransportEnv};
use hub_codegen::ir::*;
use hub_codegen::{generate_rust, generate_typescript, IR};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden").join("auth9")
}

/// Compare against the committed golden, or rewrite it under `UPDATE_GOLDEN=1`.
fn assert_golden(name: &str, actual: &str) {
    let path = golden_dir().join(name);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(golden_dir()).expect("create golden dir");
        std::fs::write(&path, actual).expect("write golden file");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("missing golden file {} ({}). Regenerate with UPDATE_GOLDEN=1 cargo test", path.display(), e)
    });
    assert_eq!(
        actual,
        expected,
        "generated output diverged from golden {} — if intentional, regenerate with UPDATE_GOLDEN=1 cargo test",
        path.display()
    );
}

fn param(name: &str, description: &str) -> ParamDef {
    ParamDef {
        pd_name: name.to_string(),
        pd_type: TypeRef::RefPrimitive("string".to_string(), None),
        pd_description: Some(description.to_string()),
        pd_required: true,
        pd_default: None,
        pd_deprecation: None,
        pd_source: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn method(
    namespace: &str,
    name: &str,
    description: &str,
    streaming: bool,
    params: Vec<ParamDef>,
    grant: Option<&str>,
    target: Option<&str>,
    long: Option<&str>,
) -> MethodDef {
    MethodDef {
        md_name: name.to_string(),
        md_full_path: format!("{}.{}", namespace, name),
        md_namespace: namespace.to_string(),
        md_description: Some(description.to_string()),
        md_streaming: streaming,
        md_params: params,
        md_returns: TypeRef::RefPrimitive("string".to_string(), None),
        md_bidir_type: None,
        md_role: Default::default(),
        md_deprecation: None,
        md_requires_credential: None,
        md_auth_posture: None,
        md_public: false,
        md_grant: grant.map(str::to_string),
        md_target: target.map(str::to_string),
        md_long_description: long.map(str::to_string),
    }
}

/// One method per namespace (Rust keeps HashMap order within a module):
/// - `tickets.set_status` — grant, target `id`, a long description;
/// - `comments.list` — a grant and no target (allowed wherever held).
/// `tickets` carries default roles.
fn fixture() -> IR {
    let mut ir_methods = HashMap::new();
    ir_methods.insert(
        "tickets.set_status".to_string(),
        method(
            "tickets",
            "set_status",
            "Change a ticket's status",
            true,
            vec![param("id", "Ticket id, e.g. MNG-2"), param("status", "New status")],
            Some("tickets.set_status"),
            Some("id"),
            Some("Moves it through Pending → Ready → Coding → In Review → Complete;\na doc has no lifecycle and is refused."),
        ),
    );
    ir_methods.insert(
        "comments.list".to_string(),
        method("comments", "list", "Comments, newest first", false, vec![], Some("comments.list"), None, None),
    );
    let mut ir_plugins = HashMap::new();
    ir_plugins.insert("tickets".to_string(), vec!["set_status".to_string()]);
    ir_plugins.insert("comments".to_string(), vec!["list".to_string()]);

    let mut roles = BTreeMap::new();
    roles.insert("viewer".to_string(), vec!["tickets.get".to_string()]);
    roles.insert("editor".to_string(), vec!["tickets.get".to_string(), "tickets.set_status".to_string()]);
    let mut ir_plugin_roles = HashMap::new();
    ir_plugin_roles.insert("tickets".to_string(), roles);

    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "menger".to_string(),
        ir_hash: Some("auth9-golden-fixture".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods,
        ir_plugins,
        ir_plugin_deprecations: HashMap::new(),
        ir_plugin_requests: HashMap::new(),
        ir_plugin_roles,
    }
}

/// The same IR as a pre-AUTH schema would give it.
fn bare_fixture() -> IR {
    let mut ir = fixture();
    for m in ir.ir_methods.values_mut() {
        m.md_grant = None;
        m.md_target = None;
        m.md_long_description = None;
    }
    ir.ir_plugin_roles.clear();
    ir
}

fn opts() -> GenerationOptions {
    GenerationOptions { transport: TransportEnv::Ws, ..GenerationOptions::default() }
}

// ─────────────────────────────────────────────────────────────
// 1 — IR decode
// ─────────────────────────────────────────────────────────────

#[test]
fn the_ir_carries_grant_target_long_description_and_roles() {
    let json = r#"{
        "irVersion": "2.0", "irBackend": "menger", "irTypes": {},
        "irMethods": {
            "tickets.set_status": {
                "mdName": "set_status", "mdFullPath": "tickets.set_status", "mdNamespace": "tickets",
                "mdDescription": "Change a ticket's status", "mdStreaming": true, "mdParams": [],
                "mdReturns": {"tag": "RefAny"},
                "mdGrant": "tickets.set_status", "mdTarget": "id",
                "mdLongDescription": "Moves it through Pending → Ready."
            },
            "tickets.list": {
                "mdName": "list", "mdFullPath": "tickets.list", "mdNamespace": "tickets",
                "mdStreaming": true, "mdParams": [], "mdReturns": {"tag": "RefAny"},
                "mdGrant": "tickets.list"
            }
        },
        "irPlugins": {"tickets": ["set_status", "list"]},
        "irPluginRoles": {"tickets": {"editor": ["tickets.list", "tickets.set_status"]}}
    }"#;
    let ir: IR = serde_json::from_str(json).expect("an AUTH-9 IR decodes");
    let m = &ir.ir_methods["tickets.set_status"];
    assert_eq!(m.md_grant.as_deref(), Some("tickets.set_status"));
    assert_eq!(m.md_target.as_deref(), Some("id"));
    assert_eq!(m.md_long_description.as_deref(), Some("Moves it through Pending → Ready."));
    assert_eq!(ir.ir_methods["tickets.list"].md_target, None);
    assert_eq!(ir.ir_plugin_roles["tickets"]["editor"], vec!["tickets.list", "tickets.set_status"]);
}

#[test]
fn an_ir_without_them_decodes() {
    for roles in ["", r#", "irPluginRoles": null"#] {
        let json = format!(
            r#"{{"irVersion": "2.0", "irBackend": "x", "irTypes": {{}},
                "irMethods": {{"a.b": {{"mdName": "b", "mdFullPath": "a.b", "mdNamespace": "a",
                  "mdStreaming": false, "mdParams": [], "mdReturns": {{"tag": "RefAny"}}}}}},
                "irPlugins": {{"a": ["b"]}}{}}}"#,
            roles
        );
        let ir: IR = serde_json::from_str(&json).expect("a pre-AUTH IR decodes");
        let m = &ir.ir_methods["a.b"];
        assert!(m.md_grant.is_none() && m.md_target.is_none() && m.md_long_description.is_none());
        assert!(ir.ir_plugin_roles.is_empty());
        // …and serializes back without the keys.
        let back = serde_json::to_string(&ir).unwrap();
        assert!(!back.contains("mdGrant") && !back.contains("irPluginRoles"));
    }
}

// ─────────────────────────────────────────────────────────────
// 2 + 3 — TypeScript
// ─────────────────────────────────────────────────────────────

#[test]
fn golden_ts_clients() {
    let result = generate_typescript(&fixture(), &opts()).expect("generate");
    assert_golden("ts_tickets_client.ts", &result.files["tickets/client.ts"]);
    assert_golden("ts_comments_client.ts", &result.files["comments/client.ts"]);
}

#[test]
fn ts_says_what_each_method_requires() {
    let result = generate_typescript(&fixture(), &opts()).expect("generate");
    let tickets = &result.files["tickets/client.ts"];
    assert!(tickets.contains(
        "  /**\n   * Change a ticket's status\n   *\n   * Moves it through Pending → Ready → Coding → In Review → Complete;\n   * a doc has no lifecycle and is refused.\n   * @requires `tickets.set_status` on the scope of `id`\n   */\n  setStatus("
    ));
    assert!(tickets.contains(
        "export const requires = {\n  \"tickets.set_status\": { grant: \"tickets.set_status\", target: \"id\" },\n} as const;"
    ));
    assert!(tickets.contains("export const defaultRoles = {"));
    let comments = &result.files["comments/client.ts"];
    assert!(comments.contains("@requires `comments.list` anywhere it is held"));
    assert!(comments.contains("\"comments.list\": { grant: \"comments.list\", target: null },"));
    assert!(!comments.contains("defaultRoles"));
}

#[test]
fn ts_throws_a_typed_forbidden() {
    let result = generate_typescript(&fixture(), &opts()).expect("generate");
    let types = &result.files["types.ts"];
    assert!(types.contains("export class Forbidden extends ForbiddenError implements Refusal {"));
    assert!(types.contains("  readonly heldOn: string[];"));
    assert!(types.contains("if (item.error_kind === 'forbidden' && typeof item.grant === 'string') {"));
    assert!(types.contains("heldOn: item.held_on ?? [],"));
    // the error item declares AUTH-2's optional fields
    assert!(types.contains("  /** Whether the operation can be retried */\n  recoverable: boolean;\n  /** AUTH-2: `forbidden` or `unauthenticated` for an auth refusal */\n  error_kind?: string;"));
    // Forbidden comes after ForbiddenError (a class must be declared before it is extended)
    assert!(types.find("export class ForbiddenError").unwrap() < types.find("export class Forbidden extends").unwrap());
    // the RPC helpers throw what errorFromStreamItem returns
    assert!(result.files["rpc.ts"].contains("throw errorFromStreamItem(item);"));
    // the transport-only selector carries it too
    let transport_only = generate_typescript(
        &fixture(),
        &GenerationOptions { generate: hub_codegen::GenerateSelector::Transport, ..opts() },
    )
    .expect("generate");
    assert!(transport_only.files["types.ts"].contains("export class Forbidden extends ForbiddenError"));
}

// ─────────────────────────────────────────────────────────────
// 4 — Rust
// ─────────────────────────────────────────────────────────────

#[test]
fn golden_rust_modules() {
    let result = generate_rust(&fixture()).expect("generate");
    assert_golden("rust_tickets_mod.rs", &result.files["src/tickets/mod.rs"]);
    assert_golden("rust_comments_mod.rs", &result.files["src/comments/mod.rs"]);
    let client = &result.files["src/client.rs"];
    assert!(client.contains("pub struct Requirement {"));
}

#[test]
fn rust_says_what_each_method_requires_and_compiles() {
    let result = generate_rust(&fixture()).expect("generate");
    let tickets = &result.files["src/tickets/mod.rs"];
    assert!(tickets.contains(
        "/// Change a ticket's status\n///\n/// Moves it through Pending → Ready → Coding → In Review → Complete;\n/// a doc has no lifecycle and is refused.\n///\n/// Requires `tickets.set_status` on the scope of `id`.\npub async fn set_status("
    ));
    assert!(tickets.contains(
        "crate::client::Requirement { method: \"set_status\", grant: \"tickets.set_status\", target: Some(\"id\") },"
    ));
    assert!(tickets.contains("pub fn requires_of(method: &str)"));
    assert!(tickets.contains("pub const DEFAULT_ROLES: &[(&str, &[&str])] = &["));

    let dir = std::env::temp_dir().join("plexus-rust-codegen-auth9");
    let _ = std::fs::remove_dir_all(&dir);
    for (path, content) in &result.files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, content).unwrap();
    }
    let out = std::process::Command::new("cargo")
        .arg("check")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .output()
        .expect("run cargo check");
    assert!(out.status.success(), "generated Rust fails to compile:\n{}", String::from_utf8_lossy(&out.stderr));
}

// ─────────────────────────────────────────────────────────────
// 5 — absence
// ─────────────────────────────────────────────────────────────

#[test]
fn without_the_fields_nothing_is_said() {
    let ts = generate_typescript(&bare_fixture(), &opts()).expect("generate");
    for (path, content) in &ts.files {
        for marker in ["@requires `", "export const requires", "defaultRoles", "class Forbidden ", "error_kind", "AUTH-9"] {
            assert!(!content.contains(marker), "{} carries {:?} without grants", path, marker);
        }
    }
    let rs = generate_rust(&bare_fixture()).expect("generate");
    for (path, content) in &rs.files {
        for marker in ["Requires `", "REQUIRES", "Requirement", "DEFAULT_ROLES", "AUTH-9"] {
            assert!(!content.contains(marker), "{} carries {:?} without grants", path, marker);
        }
    }
}

/// The bare fixture's TypeScript and Rust, byte for byte what codegen
/// before AUTH-9 produced for it (goldens written from the parent commit).
#[test]
fn without_the_fields_the_output_is_unchanged() {
    let ts = generate_typescript(&bare_fixture(), &opts()).expect("generate");
    assert_golden("bare_ts_tickets_client.ts", &ts.files["tickets/client.ts"]);
    assert_golden("bare_ts_types.ts", &ts.files["types.ts"]);
    let rs = generate_rust(&bare_fixture()).expect("generate");
    assert_golden("bare_rust_tickets_mod.rs", &rs.files["src/tickets/mod.rs"]);
    assert_golden("bare_rust_client.rs", &rs.files["src/client.rs"]);
}
