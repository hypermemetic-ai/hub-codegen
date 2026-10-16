//! SYNAPSE-CC-2: a parameter the server injects is none the client sends.
//!
//! A `ParamDef` whose `pdSource.from` is anything but `rpc` (`auth`,
//! `derived`, `header`, `cookie`, `query`) is filled by the server from the
//! connection. hub-codegen 0.4.1 still wrote it into the generated method as
//! an optional `unknown` argument — in name order, so `auth` took the first
//! positional slot ahead of every real parameter. Now:
//!
//! 1. TypeScript — an injected parameter is in neither the signature, the
//!    call body nor the params object; the JSDoc breadcrumb stays; a method
//!    with only injected parameters takes no arguments.
//! 2. Rust — the same, with the breadcrumb in the doc comment.
//! 3. A static child (`mdRole: static_child`) is a typed sub-client
//!    (`tickets.inputs.answer(...)`), not `inputs(): Promise<unknown>`; in
//!    Rust it is its module (`tickets::inputs::answer`), not a function.
//! 4. An IR without `pdSource` and without a static child generates byte
//!    for byte what 0.4.1 generated (goldens written from the parent commit).
//!
//! The fixture is the IR of the ACCESS-6 spike (`synapse -i spike`): a hub
//! `spike` with `tickets` (`get`, `set_status`) and its static child
//! `tickets.inputs` (`list`, `answer`), every method declaring `auth`
//! (derived), `origin` (header) and `user` (auth, resolver
//! `self.access.verify`).

use hub_codegen::generator::{GenerationOptions, TransportEnv};
use hub_codegen::ir::*;
use hub_codegen::{generate_rust, generate_typescript, IR};
use std::path::PathBuf;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden").join("synapse_cc_2")
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

/// The Rust generator keeps a namespace's functions in HashMap order, so a
/// module with several is compared as the set of its blank-line-separated
/// blocks (each function's doc comment and body is one block).
fn assert_golden_blocks(name: &str, actual: &str) {
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        return assert_golden(name, actual);
    }
    let expected = std::fs::read_to_string(golden_dir().join(name)).expect("golden file");
    let blocks = |s: &str| {
        let mut v: Vec<String> = s.split("\n\n").map(str::to_string).collect();
        v.sort();
        v
    };
    assert_eq!(blocks(actual), blocks(&expected), "{} diverged (compared block by block)", name);
}

fn fixture() -> IR {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synapse_cc_2_injected_ir.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("read fixture")).expect("the fixture decodes")
}

/// The fixture as a schema without injected parameters or a static child
/// would give it: every `pdSource` parameter and the `tickets.inputs`
/// accessor removed (what menger's `sdk-codegen.sh` filter sent). Without
/// its metadata, so types.ts carries no toolchain stamp (the version moves).
fn bare_fixture() -> IR {
    let mut ir = fixture();
    ir.ir_metadata = None;
    for m in ir.ir_methods.values_mut() {
        m.md_params.retain(|p| p.pd_source.is_none());
    }
    ir.ir_methods.remove("tickets.inputs");
    if let Some(ms) = ir.ir_plugins.get_mut("tickets") {
        ms.retain(|m| m != "inputs");
    }
    ir
}

fn opts() -> GenerationOptions {
    GenerationOptions { transport: TransportEnv::Ws, ..GenerationOptions::default() }
}

// ─────────────────────────────────────────────────────────────
// the IR
// ─────────────────────────────────────────────────────────────

#[test]
fn injected_is_any_source_but_rpc() {
    let ir = fixture();
    let get = &ir.ir_methods["tickets.get"];
    let injected: Vec<&str> = get.md_params.iter().filter(|p| p.is_injected()).map(|p| p.pd_name.as_str()).collect();
    assert_eq!(injected, ["auth", "origin", "user"]);
    let sent: Vec<String> = get.sent_params().into_iter().map(|p| p.pd_name).collect();
    assert_eq!(sent, ["id"]);

    let with = |source: serde_json::Value| ParamDef {
        pd_name: "x".into(),
        pd_type: TypeRef::RefAny,
        pd_description: None,
        pd_required: false,
        pd_default: None,
        pd_deprecation: None,
        pd_source: Some(source),
    };
    assert!(!with(serde_json::json!({ "from": "rpc" })).is_injected());
    for from in ["auth", "derived", "header", "cookie", "query", "peer"] {
        assert!(with(serde_json::json!({ "from": from })).is_injected(), "{from}");
    }
    assert!(with(serde_json::json!({})).is_injected(), "a source without `from` is still not the caller");
    assert!(!ParamDef { pd_source: None, ..with(serde_json::json!({})) }.is_injected());
}

// ─────────────────────────────────────────────────────────────
// 1 + 3 — TypeScript
// ─────────────────────────────────────────────────────────────

#[test]
fn golden_ts_clients() {
    let ts = generate_typescript(&fixture(), &opts()).expect("generate");
    assert_golden("ts_tickets_client.ts", &ts.files["tickets/client.ts"]);
    assert_golden("ts_tickets_inputs_client.ts", &ts.files["tickets/inputs/client.ts"]);
}

#[test]
fn ts_leaves_injected_params_out_and_keeps_the_breadcrumbs() {
    let ts = generate_typescript(&fixture(), &opts()).expect("generate");
    let tickets = &ts.files["tickets/client.ts"];
    let inputs = &ts.files["tickets/inputs/client.ts"];
    for file in [tickets, inputs] {
        for gone in ["auth?:", "origin?:", "user?:", "auth, ", ", origin", ", user", "?: unknown"] {
            assert!(!file.contains(gone), "{gone:?} is still generated:\n{file}");
        }
    }
    // signatures and params objects: the caller's own parameters, in order
    assert!(tickets.contains("  setStatus(id: TicketId, status: Status): AsyncGenerator<SpikeEvent>;"));
    assert!(tickets.contains("  async *setStatus(id: TicketId, status: Status): AsyncGenerator<SpikeEvent> {"));
    assert!(tickets.contains("this.rpc.call('tickets.set_status', { id, status });"));
    assert!(tickets.contains("  get(id: TicketId): AsyncGenerator<SpikeEvent>;"));
    assert!(tickets.contains("this.rpc.call('tickets.get', { id });"));
    assert!(inputs.contains("  answer(ticket: TicketId, value: Status): AsyncGenerator<SpikeEvent>;"));
    assert!(inputs.contains("this.rpc.call('tickets.inputs.answer', { ticket, value });"));
    // the breadcrumbs stay
    for crumb in ["@server-derived auth", "@reads-header origin", "@requiresAuth — resolver: self.access.verify"] {
        assert!(tickets.contains(crumb) && inputs.contains(crumb), "{crumb} lost");
    }
}

#[test]
fn ts_a_method_with_only_injected_params_takes_no_arguments() {
    let mut ir = fixture();
    ir.ir_methods.get_mut("tickets.get").unwrap().md_params.retain(|p| p.is_injected());
    let ts = generate_typescript(&ir, &opts()).expect("generate");
    let tickets = &ts.files["tickets/client.ts"];
    assert!(tickets.contains("  get(): AsyncGenerator<SpikeEvent>;"), "{tickets}");
    assert!(tickets.contains("  async *get(): AsyncGenerator<SpikeEvent> {"));
    assert!(tickets.contains("this.rpc.call('tickets.get', {});"));
    assert!(tickets.contains("@requiresAuth — resolver: self.access.verify"));
}

#[test]
fn ts_a_static_child_is_a_typed_sub_client() {
    let ts = generate_typescript(&fixture(), &opts()).expect("generate");
    let tickets = &ts.files["tickets/client.ts"];
    assert!(!tickets.contains("inputs(): Promise<unknown>"), "{tickets}");
    assert!(!tickets.contains("'tickets.inputs', {}"));
    assert!(tickets.contains("import type { TicketsInputsClient } from '../tickets/inputs/client';"));
    assert!(tickets.contains("import { createTicketsInputsClient } from '../tickets/inputs/client';"));
    assert!(tickets.contains("  /** The inputs namespace — `tickets.inputs.<method>`. */\n  readonly inputs: TicketsInputsClient;"));
    assert!(tickets.contains("    this.inputs = createTicketsInputsClient(rpc);"));
}

// ─────────────────────────────────────────────────────────────
// 2 + 3 — Rust
// ─────────────────────────────────────────────────────────────

#[test]
fn rust_leaves_injected_params_out_and_keeps_the_breadcrumbs() {
    let rs = generate_rust(&fixture()).expect("generate");
    let tickets = &rs.files["src/tickets/mod.rs"];
    let inputs = &rs.files["src/tickets/inputs/mod.rs"];
    for file in [tickets, inputs] {
        for gone in ["auth: serde_json::Value", "origin: serde_json::Value", "user: serde_json::Value", "\"auth\":", "\"user\":"] {
            assert!(!file.contains(gone), "{gone:?} is still generated:\n{file}");
        }
    }
    assert!(tickets.contains("pub async fn set_status(client: &PlexusClient, id: TicketId, status: Status) -> "));
    assert!(tickets.contains("client.call_stream(\"tickets.set_status\", json!({ \"id\": id, \"status\": status }))"));
    assert!(inputs.contains("pub async fn answer(client: &PlexusClient, ticket: TicketId, value: Status) -> "));
    assert!(tickets.contains(
        "/// Injected by the server, never sent:\n/// - `@server-derived auth`\n/// - `@reads-header origin`\n/// - `@requiresAuth — resolver: self.access.verify`\npub async fn set_status("
    ));
}

#[test]
fn rust_a_static_child_is_its_module() {
    let rs = generate_rust(&fixture()).expect("generate");
    let tickets = &rs.files["src/tickets/mod.rs"];
    assert!(tickets.contains("pub mod inputs;"));
    assert!(!tickets.contains("pub async fn inputs("), "{tickets}");
    assert!(!tickets.contains("\"tickets.inputs\""));
}

/// A small IR the Rust target can compile (the spike's `Status` has a
/// variant with a space, which the Rust generator does not yet rename):
/// one method with a real and an injected parameter, one with only
/// injected ones, and a static child.
#[test]
fn rust_with_injected_params_compiles() {
    let json = r#"{
      "irVersion": "2.0", "irBackend": "svc", "irTypes": {},
      "irPlugins": { "svc": ["ping", "whoami", "kids"], "svc.kids": ["count"] },
      "irMethods": {
        "svc.ping": { "mdName": "ping", "mdFullPath": "svc.ping", "mdNamespace": "svc", "mdStreaming": false,
          "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]},
          "mdParams": [
            {"pdName": "auth", "pdType": {"tag": "RefAny"}, "pdRequired": false, "pdSource": {"from": "derived"}},
            {"pdName": "text", "pdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "pdRequired": true}
          ] },
        "svc.whoami": { "mdName": "whoami", "mdFullPath": "svc.whoami", "mdNamespace": "svc", "mdStreaming": true,
          "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]},
          "mdParams": [
            {"pdName": "user", "pdType": {"tag": "RefAny"}, "pdRequired": false, "pdSource": {"from": "auth", "resolver": "self.verify"}}
          ] },
        "svc.kids": { "mdName": "kids", "mdFullPath": "svc.kids", "mdNamespace": "svc", "mdStreaming": false,
          "mdReturns": {"tag": "RefUnknown"}, "mdParams": [], "mdRole": {"kind": "static_child"} },
        "svc.kids.count": { "mdName": "count", "mdFullPath": "svc.kids.count", "mdNamespace": "svc.kids", "mdStreaming": false,
          "mdReturns": {"tag": "RefPrimitive", "contents": ["integer", null]},
          "mdParams": [
            {"pdName": "origin", "pdType": {"tag": "RefAny"}, "pdRequired": false, "pdSource": {"from": "header", "key": "origin"}}
          ] }
      }
    }"#;
    let ir: IR = serde_json::from_str(json).expect("decode");
    let rs = generate_rust(&ir).expect("generate");
    let svc = &rs.files["src/svc/mod.rs"];
    assert!(svc.contains("pub async fn ping(client: &PlexusClient, text: String) -> Result<String> {"), "{svc}");
    assert!(svc.contains("client.call_single(\"svc.ping\", json!({ \"text\": text })).await"));
    assert!(svc.contains("pub async fn whoami(client: &PlexusClient) -> "));
    assert!(svc.contains("client.call_stream(\"svc.whoami\", serde_json::Value::Null)"));
    assert!(!svc.contains("pub async fn kids("));
    assert!(rs.files["src/svc/kids/mod.rs"].contains("pub async fn count(client: &PlexusClient) -> "));

    let ts = generate_typescript(&ir, &opts()).expect("generate");
    let svc_ts = &ts.files["svc/client.ts"];
    assert!(svc_ts.contains("  whoami(): AsyncGenerator<string>;"), "{svc_ts}");
    assert!(svc_ts.contains("  ping(text: string): Promise<string>;"));
    assert!(svc_ts.contains("  readonly kids: SvcKidsClient;"));

    let dir = std::env::temp_dir().join("plexus-rust-codegen-synapse-cc-2");
    let _ = std::fs::remove_dir_all(&dir);
    for (path, content) in &rs.files {
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
// 4 — absence: byte for byte as before
// ─────────────────────────────────────────────────────────────

/// The bare fixture generates exactly what hub-codegen 0.4.1 generated for
/// it (goldens written from the parent commit, fe42998).
#[test]
fn without_injected_params_the_output_is_unchanged() {
    let ir = bare_fixture();
    let ts = generate_typescript(&ir, &opts()).expect("generate");
    for (golden, file) in [
        ("bare_ts_tickets_client.ts", "tickets/client.ts"),
        ("bare_ts_tickets_inputs_client.ts", "tickets/inputs/client.ts"),
        ("bare_ts_tickets_types.ts", "tickets/types.ts"),
        ("bare_ts_types.ts", "types.ts"),
        ("bare_ts_rpc.ts", "rpc.ts"),
        ("bare_ts_index.ts", "index.ts"),
    ] {
        assert_golden(golden, &ts.files[file]);
    }
    let rs = generate_rust(&ir).expect("generate");
    assert_golden_blocks("bare_rust_tickets_mod.rs", &rs.files["src/tickets/mod.rs"]);
    assert_golden_blocks("bare_rust_tickets_inputs_mod.rs", &rs.files["src/tickets/inputs/mod.rs"]);
    assert_golden("bare_rust_client.rs", &rs.files["src/client.rs"]);
}
