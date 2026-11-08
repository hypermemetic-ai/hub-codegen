//! ASK-4 · a method that asks (ASK-1) in the generated clients, and bidir gone.
//!
//! A method whose IR says `mdAsks: true` may finish its call with a proposal
//! (`needs_input`) in place of acting; the caller calls it again with the same
//! params and `confirm`. So such a method takes an optional
//! `confirm: ConfirmToken` and its item gains `NeedsInput`:
//!
//! - TypeScript: `confirm?: ConfirmToken`, the item `T | NeedsInput`, the
//!   aggregate's member too; `ConfirmToken`, `NeedsInputField`, `NeedsInput`
//!   and `isNeedsInput` once, in the package's `types.ts`.
//! - Rust: `confirm: Option<ConfirmToken>`, the item `Asking<T>` (`Item(T)` or
//!   `NeedsInput(NeedsInput)`); the standard types once, in `src/types.rs`.
//!
//! An older IR's bidir (`mdBidirType`, `bidirectional`) reads as not asking
//! and generates what an IR without it does. Fixture:
//! `tests/fixtures/ask4_asks_ir.json` (`tickets.archive` streams and asks,
//! `tickets.delete` answers once and asks, `tickets.get` asks nothing);
//! goldens in `golden/ask4/`; the TypeScript tests in `ts_ask4/`.

use service_codegen::generator::{GenerationOptions, TransportEnv};
use service_codegen::{generate_rust, generate_typescript, IR};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_json() -> serde_json::Value {
    let path = root().join("tests/fixtures/ask4_asks_ir.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("read fixture")).expect("fixture json")
}

fn ir_of(v: &serde_json::Value) -> IR {
    serde_json::from_value(v.clone()).expect("the IR decodes")
}

fn ts(ir: &IR, transport: TransportEnv) -> HashMap<String, String> {
    generate_typescript(ir, &GenerationOptions { transport, ..GenerationOptions::default() })
        .expect("generate typescript")
        .files
}

fn rust(ir: &IR) -> HashMap<String, String> {
    generate_rust(ir).expect("generate rust").files
}

fn golden(name: &str, actual: &str) {
    let path = root().join("tests/golden/ask4").join(name);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("golden {name} (UPDATE_GOLDEN=1 writes it)"));
    assert_eq!(actual, expected, "{name} diverged from its golden (UPDATE_GOLDEN=1 to accept)");
}

fn run(cmd: &mut Command) -> (bool, String) {
    let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn write_all(dir: &Path, files: &HashMap<String, String>) {
    for (path, content) in files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
}

/// Each method of the fixture with `mdAsks` taken out, and `rewrite` applied.
fn without_asks(rewrite: impl Fn(&mut serde_json::Map<String, serde_json::Value>)) -> serde_json::Value {
    let mut v = fixture_json();
    for m in v["irMethods"].as_object_mut().unwrap().values_mut() {
        let m = m.as_object_mut().unwrap();
        m.remove("mdAsks");
        // the IR's own `confirm` is the asking method's; without asking it is not there
        if let Some(serde_json::Value::Array(ps)) = m.get_mut("mdParams") {
            ps.retain(|p| p["pdName"] != "confirm");
        }
        rewrite(m);
    }
    v
}

// ─────────────────────────────────────────────────────────────
// 1. TypeScript
// ─────────────────────────────────────────────────────────────

#[test]
fn ts_goldens() {
    let files = ts(&ir_of(&fixture_json()), TransportEnv::Browser);
    golden("ts_tickets_client.ts", &files["tickets/client.ts"]);
    golden("ts_tickets_events.ts", &files["tickets/events.ts"]);
}

#[test]
fn ts_an_asking_method_takes_confirm_and_may_answer_needs_input() {
    let files = ts(&ir_of(&fixture_json()), TransportEnv::Browser);
    let client = &files["tickets/client.ts"];
    for line in [
        "import type { ConfirmToken, NeedsInput } from '../types';",
        "  archive(filter: string, confirm?: ConfirmToken): MethodStream<'tickets.archive', ArchiveEvent | NeedsInput, Up<A, TicketsEvent>>;",
        "  delete(ids: string[], children?: string, confirm?: ConfirmToken): Promise<Deleted | NeedsInput>;",
        "  get(id: string): Promise<Deleted>;",
        "    const stream = this.rpc.call('tickets.archive', { filter, confirm });",
        "    return collectOne<Deleted | NeedsInput>(stream);",
    ] {
        assert!(client.contains(line), "{line}\n{client}");
    }
    // the IR's own `confirm` (a plain string there) is the standard one, once
    assert_eq!(client.matches("confirm?: ConfirmToken").count(), 4, "{client}");
    assert!(!client.contains("confirm?: string"), "{client}");

    let events = &files["tickets/events.ts"];
    assert!(events.contains("  | { method: 'archive'; event: ArchiveEvent | NeedsInput }"), "{events}");
    assert!(events.contains("  | { method: 'get'; event: Deleted };"), "{events}");
    assert!(events.contains("import type { NeedsInput } from '../types';"), "{events}");
}

#[test]
fn ts_the_standard_types_are_emitted_once_per_package() {
    for transport in [TransportEnv::Ws, TransportEnv::Browser, TransportEnv::None] {
        let files = ts(&ir_of(&fixture_json()), transport);
        let decl = |name: &str| -> usize { files.values().map(|c| c.matches(name).count()).sum() };
        assert_eq!(decl("export type ConfirmToken = string;"), 1, "{transport:?}");
        assert_eq!(decl("export interface NeedsInput {"), 1, "{transport:?}");
        assert_eq!(decl("export interface NeedsInputField {"), 1, "{transport:?}");
        assert_eq!(decl("export function isNeedsInput("), 1, "{transport:?}");
        let types = &files["types.ts"];
        for f in ["  type: 'needs_input';", "  summary: string;", "  effects: unknown[];", "  fields: NeedsInputField[];", "  token: ConfirmToken;", "  expires: string;", "  name: string;", "  options?: unknown[];", "  default?: unknown;", "  help?: string;"] {
            assert!(types.contains(f), "{transport:?}: {f}");
        }
        // index.ts re-exports types.ts whole: the standard types are the package's
        assert!(files["index.ts"].contains("export * from './types';"));
    }
}

#[test]
fn bidir_is_gone_from_every_typescript_file() {
    for transport in [TransportEnv::Ws, TransportEnv::Browser, TransportEnv::None] {
        let files = ts(&ir_of(&fixture_json()), transport);
        for (path, content) in &files {
            for gone in ["StandardRequest", "StandardResponse", "PlexusStreamItemRequest", "Bidirectional", ".respond`", "bidir-smoke", "test:bidir"] {
                assert!(!content.contains(gone), "{transport:?} {path}: {gone}");
            }
        }
        assert!(!files.contains_key("test/bidir-smoke.test.ts"));
    }
}

// ─────────────────────────────────────────────────────────────
// 2. Rust
// ─────────────────────────────────────────────────────────────

#[test]
fn rust_goldens() {
    let files = rust(&ir_of(&fixture_json()));
    golden("rust_tickets_mod.rs", &files["src/tickets/mod.rs"]);
}

#[test]
fn rust_an_asking_method_takes_confirm_and_may_answer_needs_input() {
    let files = rust(&ir_of(&fixture_json()));
    let m = &files["src/tickets/mod.rs"];
    for line in [
        "pub async fn archive(client: &PlexusClient, filter: String, confirm: Option<ConfirmToken>) -> Result<Pin<Box<dyn Stream<Item = Result<Asking<ArchiveEvent>>> + Send>>> {",
        "pub async fn delete(client: &PlexusClient, ids: Vec<String>, children: String, confirm: Option<ConfirmToken>) -> Result<Asking<Deleted>> {",
        "pub async fn get(client: &PlexusClient, id: String) -> Result<Deleted> {",
        "json!({ \"filter\": filter, \"confirm\": confirm })",
    ] {
        assert!(m.contains(line), "{line}\n{m}");
    }
    let types = &files["src/types.rs"];
    for decl in ["pub type ConfirmToken = String;", "pub struct NeedsInputField {", "pub struct NeedsInput {", "pub enum Asking<T> {"] {
        assert_eq!(files.values().map(|c| c.matches(decl).count()).sum::<usize>(), 1, "{decl}");
        assert!(types.contains(decl), "{decl}");
    }
}

/// The generated crate compiles, and `Asking<T>` reads a proposal as
/// `NeedsInput` and anything else as the method's item.
#[test]
fn rust_compiles_and_decodes_a_proposal() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("ask4");
    write_all(&dir, &rust(&ir_of(&fixture_json())));
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(
        dir.join("tests/asking.rs"),
        r#"
use plexus_client::tickets::{ArchiveEvent, Deleted};
use plexus_client::types::{Asking, NeedsInput, NeedsInputField};
use serde_json::json;

#[test]
fn a_proposal_is_needs_input_and_an_item_is_the_item() {
    let proposal = json!({
        "type": "needs_input", "summary": "archive KILN-2", "effects": [{"archive": "KILN-2"}],
        "fields": [{"name": "children", "options": ["reparent", "delete"], "default": "reparent"}],
        "token": "v1.1794100000.abc", "expires": "2026-11-07T12:00:00Z"
    });
    let got: Asking<ArchiveEvent> = serde_json::from_value(proposal.clone()).unwrap();
    let want = NeedsInput {
        summary: "archive KILN-2".into(),
        effects: vec![json!({"archive": "KILN-2"})],
        fields: vec![NeedsInputField { name: "children".into(), options: Some(vec![json!("reparent"), json!("delete")]), default: Some(json!("reparent")), help: None }],
        token: "v1.1794100000.abc".into(),
        expires: "2026-11-07T12:00:00Z".into(),
    };
    match &got {
        Asking::NeedsInput(p) => assert_eq!(p, &want),
        other => panic!("{other:?}"),
    }
    // and back to the wire as it came
    assert_eq!(serde_json::to_value(&got).unwrap(), proposal);
    // a plain confirmation: no fields, no effects given
    let bare: Asking<Deleted> = serde_json::from_value(json!({"type": "needs_input", "summary": "s", "token": "t", "expires": "e"})).unwrap();
    assert!(matches!(bare, Asking::NeedsInput(NeedsInput { ref fields, .. }) if fields.is_empty()));

    let item: Asking<ArchiveEvent> = serde_json::from_value(json!({"type": "archived", "id": "KILN-2"})).unwrap();
    assert!(matches!(item, Asking::Item(ArchiveEvent::Archived { ref id }) if id == "KILN-2"), "{item:?}");
    let item: Asking<Deleted> = serde_json::from_value(json!({"ids": ["KILN-2"]})).unwrap();
    assert!(matches!(item, Asking::Item(Deleted { ref ids }) if ids == &["KILN-2"]), "{item:?}");
    // neither: the item's own error, never a quiet proposal
    assert!(serde_json::from_value::<Asking<Deleted>>(json!({"type": "needs_input", "summary": 1})).is_err());
}
"#,
    )
    .unwrap();
    let (ok, text) = run(Command::new("cargo").arg("test").arg("--manifest-path").arg(dir.join("Cargo.toml")));
    assert!(ok, "the generated crate's test failed:\n{text}");
    assert!(text.contains("test a_proposal_is_needs_input_and_an_item_is_the_item ... ok"), "{text}");
}

// ─────────────────────────────────────────────────────────────
// 3. Reading: an older IR's bidir is not asking; the next IR's number
// ─────────────────────────────────────────────────────────────

#[test]
fn an_older_irs_bidir_generates_what_no_bidir_does() {
    let plain = ir_of(&without_asks(|_| {}));
    let bidir = ir_of(&without_asks(|m| {
        m.insert("mdBidirType".into(), serde_json::json!({ "tag": "RefAny" }));
        m.insert("mdBidirResponseType".into(), serde_json::Value::Null);
        m.insert("mdBidirResponseSchema".into(), serde_json::Value::Null);
        m.insert("bidirectional".into(), serde_json::json!(true));
    }));
    assert!(bidir.ir_methods.values().all(|m| !m.md_asks));
    for t in [TransportEnv::Ws, TransportEnv::Browser] {
        assert_eq!(ts(&plain, t), ts(&bidir, t), "{t:?}");
        assert!(!ts(&bidir, t)["tickets/client.ts"].contains("NeedsInput"));
    }
    assert_eq!(rust(&plain), rust(&bidir));
    assert!(!rust(&bidir)["src/tickets/mod.rs"].contains("Asking<"));
}

#[test]
fn asks_is_read_from_ir_3_0_3_1_and_4_0_and_as_asks() {
    let want = ts(&ir_of(&fixture_json()), TransportEnv::Browser);
    for v in ["3.1", "4.0"] {
        let mut j = fixture_json();
        j["irVersion"] = serde_json::json!(v);
        let got = ts(&ir_of(&j), TransportEnv::Browser);
        // .codegen-metadata.json records the IR's version; every generated file is the same
        let differ: Vec<&String> = want.keys().filter(|k| *k != ".codegen-metadata.json" && got.get(*k) != want.get(*k)).collect();
        assert!(differ.is_empty(), "{v}: {differ:?}");
    }
    let spelled = without_asks(|m| {
        if m["mdName"] != "get" {
            m.insert("asks".into(), serde_json::json!(true));
        }
    });
    let got = ts(&ir_of(&spelled), TransportEnv::Browser);
    assert_eq!(got["tickets/client.ts"], want["tickets/client.ts"]);
}

// ─────────────────────────────────────────────────────────────
// 4. The TypeScript tests: tsc --strict and node --test
// ─────────────────────────────────────────────────────────────

/// A generated package with `ts_ask4/` beside it, compiled to CommonJS by
/// `tsc` (which also type-checks `types.test.ts`). `None` when there is no
/// Node (and it is not required).
fn compiled() -> Option<(tempfile::TempDir, PathBuf)> {
    let node_ok = Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !node_ok {
        assert!(std::env::var("SERVICE_CODEGEN_REQUIRE_TS").is_err(), "SERVICE_CODEGEN_REQUIRE_TS=1 and there is no node");
        eprintln!("node not available; skipping the TypeScript ASK-4 tests");
        return None;
    }
    let toolchain = root().join("tests/ts_watch");
    if !toolchain.join("node_modules/.bin/tsc").exists() {
        let (ok, text) = run(Command::new("npm").args(["ci", "--no-audit", "--no-fund"]).current_dir(&toolchain));
        assert!(ok, "npm ci in {}: {text}", toolchain.display());
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = tmp.path().join("pkg");
    write_all(&pkg, &ts(&ir_of(&fixture_json()), TransportEnv::Browser));
    let tests = pkg.join("ask4-tests");
    std::fs::create_dir_all(&tests).unwrap();
    for f in ["types.test.ts", "runtime.test.ts"] {
        std::fs::copy(root().join("tests/ts_ask4").join(f), tests.join(f)).unwrap();
    }
    std::os::unix::fs::symlink(toolchain.join("node_modules"), pkg.join("node_modules")).unwrap();
    std::fs::write(
        pkg.join("tsconfig.ask4.json"),
        r#"{
  "compilerOptions": {
    "target": "ES2022", "module": "commonjs", "moduleResolution": "node10",
    "lib": ["ES2022", "DOM"], "types": ["node"], "strict": true,
    "esModuleInterop": true, "skipLibCheck": true, "outDir": "out"
  },
  "include": ["*.ts", "tickets/**/*.ts", "ask4-tests/*.ts"]
}"#,
    )
    .unwrap();
    let (ok, text) = run(Command::new(toolchain.join("node_modules/.bin/tsc")).args(["-p", "tsconfig.ask4.json"]).current_dir(&pkg));
    assert!(ok, "tsc rejected the generated package or the ASK-4 tests:\n{text}");
    std::fs::write(pkg.join("out/package.json"), r#"{ "type": "commonjs" }"#).unwrap();
    Some((tmp, pkg))
}

#[test]
fn ts_types_and_runtime() {
    let Some((_tmp, pkg)) = compiled() else { return };
    let (ok, text) = run(Command::new("node").args(["--test", "--test-reporter=spec"]).arg(pkg.join("out/ask4-tests/runtime.test.js")).current_dir(&pkg));
    println!("{text}");
    assert!(ok, "node --test runtime.test.js failed:\n{text}");
    assert!(text.contains("pass 4"), "4 runtime tests:\n{text}");
}

/// Every `@ts-expect-error` is real: unmarked, `tsc` fails on exactly those lines.
#[test]
fn ts_expected_errors_are_errors() {
    let Some((_tmp, pkg)) = compiled() else { return };
    let types = pkg.join("ask4-tests/types.test.ts");
    let src = std::fs::read_to_string(&types).unwrap();
    let marked = src.matches("// @ts-expect-error").count();
    assert!(marked >= 3, "{marked} expected errors");
    std::fs::write(&types, src.replace("// @ts-expect-error", "// (unmarked)")).unwrap();
    let toolchain = root().join("tests/ts_watch");
    let (ok, text) = run(Command::new(toolchain.join("node_modules/.bin/tsc")).args(["-p", "tsconfig.ask4.json", "--noEmit"]).current_dir(&pkg));
    assert!(!ok, "unmarked, the type tests must fail");
    let errors = text.lines().filter(|l| l.contains("ask4-tests/types.test.ts") && l.contains("error TS")).count();
    assert_eq!(errors, marked, "one error per marked line:\n{text}");
}
