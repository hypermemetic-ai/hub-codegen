//! RETURNS-15 · service-codegen reads IR 3.0, and still reads IR 2.x.
//!
//! IR 3.0 (RETURNS-13, synapse after RETURNS-36) carries each method's whole
//! contract: `mdReturns` is the item `T`, `mdError` / `mdProgress` its error
//! and progress types, `mdCardinality` `"one"` or `"many"`, and no
//! `mdStreaming`. The generators branch on `md_cardinality` (the shim: where
//! they read `md_streaming`, they read `Many`), so a method generates as it
//! did; typing the error and progress is RETURNS-17 / RETURNS-18's.
//!
//! 1. `fixtures/returns15_conformance_ir.json` is RETURNS-13's
//!    `expected/ir.json` (plexus-macros@3b4b618,
//!    `fixtures/conformance/expected/ir.json`), with four doc comments copied
//!    from plexus-core spelled as NAMING-10 spells them: it decodes, its
//!    methods' cardinality, error and progress are read, and its TypeScript
//!    client type-checks under `tsc` and its Rust client under `cargo check`.
//! 2. An IR 2.x method reads as it did: `mdStreaming: true` is `Many`, absent
//!    or false is `One`, and the same method written either way generates
//!    the same client.
//! 3. An IR whose version is none of 2.0, 2.1, 3.0 is refused, naming what
//!    is read.
//! 4. The forms IR 2.1 added, which a 3.0 from synapse's main carries too
//!    (`fixtures/returns15_forms_ir.json`): `RefJson` is the JSON type,
//!    `RefMap` a string-keyed map, `RefTuple` a tuple, `RefRefined` its base,
//!    `keRepr` external and untagged enums with `vdRef`, and `mdInjected` a
//!    param the server supplies — documented, never sent. Its TypeScript
//!    type-checks and its Rust compiles.
//!
//! The `tsc` checks need Node and `tests/ts_watch`'s pinned TypeScript
//! (`npm ci` there when it is missing); without Node they are skipped unless
//! `SERVICE_CODEGEN_REQUIRE_TS=1`. The `cargo check`s need the crates the
//! generated Cargo.toml names, as `rust_codegen_smoke_test` does.

use service_codegen::generator::{GenerationOptions, TransportEnv};
use service_codegen::ir::{check_ir_version, Cardinality, EnumRepr, MethodDef, TypeKind, TypeRef};
use service_codegen::{generate_rust, generate_typescript, IR};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> IR {
    let path = root().join("tests/fixtures").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("read fixture")).expect("the fixture decodes")
}

fn ts(ir: &IR) -> HashMap<String, String> {
    generate_typescript(ir, &GenerationOptions { transport: TransportEnv::Browser, ..GenerationOptions::default() })
        .expect("generate typescript")
        .files
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

/// `tsc --noEmit` over a generated package, strict; `false` when there is no
/// Node (and it is not required).
fn type_checks(files: &HashMap<String, String>) -> bool {
    let node_ok = Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !node_ok {
        assert!(std::env::var("SERVICE_CODEGEN_REQUIRE_TS").is_err(), "SERVICE_CODEGEN_REQUIRE_TS=1 and there is no node");
        eprintln!("node not available; skipping tsc");
        return false;
    }
    let h = root().join("tests/ts_watch");
    if !h.join("node_modules/.bin/tsc").exists() {
        let (ok, text) = run(Command::new("npm").args(["ci", "--no-audit", "--no-fund"]).current_dir(&h));
        assert!(ok, "npm ci in {}: {text}", h.display());
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = tmp.path().join("pkg");
    write_all(&pkg, files);
    std::os::unix::fs::symlink(h.join("node_modules"), pkg.join("node_modules")).unwrap();
    std::fs::write(
        pkg.join("tsconfig.check.json"),
        r#"{
  "compilerOptions": {
    "target": "ES2022", "module": "commonjs", "moduleResolution": "node10",
    "lib": ["ES2022", "DOM"], "types": ["node"], "strict": true,
    "esModuleInterop": true, "skipLibCheck": true, "noEmit": true
  },
  "include": ["**/*.ts"],
  "exclude": ["node_modules", "test"]
}"#,
    )
    .unwrap();
    let (ok, text) = run(Command::new(h.join("node_modules/.bin/tsc")).args(["-p", "tsconfig.check.json"]).current_dir(&pkg));
    assert!(ok, "tsc rejected the generated package:\n{text}");
    true
}

/// `cargo check` over a generated Rust client.
fn cargo_checks(ir: &IR, name: &str) {
    let files = generate_rust(ir).expect("generate rust").files;
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join(name);
    write_all(&dir, &files);
    let (ok, text) = run(Command::new("cargo").arg("check").arg("--manifest-path").arg(dir.join("Cargo.toml")));
    assert!(ok, "cargo check rejected the generated {name} client:\n{text}");
}

fn method<'a>(ir: &'a IR, path: &str) -> &'a MethodDef {
    ir.ir_methods.get(path).unwrap_or_else(|| panic!("{path}"))
}

fn named_local(t: &Option<TypeRef>) -> Option<String> {
    match t {
        Some(TypeRef::RefNamed(q)) => Some(q.qn_local_name.clone()),
        Some(other) => panic!("not a named ref: {other:?}"),
        None => None,
    }
}

// ─────────────────────────────────────────────────────────────
// 1. RETURNS-13's IR 3.0
// ─────────────────────────────────────────────────────────────

#[test]
fn conformance_ir3_decodes_with_each_methods_contract() {
    let ir = fixture("returns15_conformance_ir");
    assert_eq!(ir.ir_version, "3.0");
    let expect = [
        ("count", Cardinality::Many, None, None),
        ("load", Cardinality::Many, Some("LoadError"), Some("LoadStep")),
        ("lookup", Cardinality::One, Some("LookupError"), None),
        ("ping", Cardinality::One, None, None),
        ("scan", Cardinality::Many, Some("ScanError"), None),
        ("schema", Cardinality::One, None, None),
        ("warm", Cardinality::Many, None, Some("Note")),
    ];
    for (name, card, err, prog) in expect {
        let m = method(&ir, &format!("conformance.{name}"));
        assert_eq!(m.md_cardinality, card, "{name}");
        assert_eq!(m.is_streaming(), card == Cardinality::Many, "{name}");
        assert_eq!(named_local(&m.md_error).as_deref(), err, "{name} error");
        assert_eq!(named_local(&m.md_progress).as_deref(), prog, "{name} progress");
        // every named T, E, P has its TypeDef (RETURNS-13: no dangling ref)
        for t in [Some(&m.md_returns), m.md_error.as_ref(), m.md_progress.as_ref()].into_iter().flatten() {
            if let TypeRef::RefNamed(q) = t {
                assert!(ir.ir_types.contains_key(&q.full_name()), "{name}: {} has no TypeDef", q.full_name());
            }
        }
    }
}

#[test]
fn conformance_ir3_generates_by_cardinality() {
    let ir = fixture("returns15_conformance_ir");
    let files = ts(&ir);
    let client = &files["conformance/client.ts"];
    for (name, item) in [("count", "Tick"), ("load", "Loaded"), ("scan", "Row"), ("warm", "Warmed")] {
        assert!(client.contains(&format!("MethodStream<'conformance.{name}', {item}, Up<A, ConformanceEvent>>")), "{name}:\n{client}");
    }
    for (sig, item) in [("lookup(id: string)", "Thing"), ("ping()", "Pong")] {
        assert!(client.contains(&format!("  {sig}: Promise<{item}>;")), "{sig}:\n{client}");
    }
    let types = &files["conformance/types.ts"];
    for t in ["LoadError", "LoadStep", "LookupError", "ScanError", "Note"] {
        assert!(types.contains(&format!("export type {t} =")) || types.contains(&format!("export interface {t} ")), "{t}:\n{types}");
    }

    let rust = generate_rust(&ir).expect("rust").files;
    let module = &rust["src/conformance/mod.rs"];
    assert!(module.contains("pub async fn lookup(client: &PlexusClient, id: String) -> Result<Thing> {"), "{module}");
    assert!(module.contains("pub async fn scan(client: &PlexusClient, prefix: String) -> Result<Pin<Box<dyn Stream<Item = Result<Row>> + Send>>> {"), "{module}");
}

#[test]
fn conformance_ir3_typescript_type_checks() {
    type_checks(&ts(&fixture("returns15_conformance_ir")));
}

#[test]
fn conformance_ir3_rust_compiles() {
    cargo_checks(&fixture("returns15_conformance_ir"), "conformance");
}

/// The same fixture's IR as the merged synapse builds it (synapse 4.0.0 at
/// 645688b, RETURNS-36's merge: IR 3.0 over IR 2.1's reader, via
/// `buildIRFromSchema` on plexus-macros returns/0.7's `expected/schema.json`,
/// the same four doc comments respelled): `SchemaResult` is an untagged enum of
/// `PluginSchema | MethodSchema`, and `irRefusals` names one site
/// (`conformance.Schema`, a JSON Schema type list), typed `unknown` here.
#[test]
fn synapse4_conformance_ir3_generates_and_checks() {
    let ir = fixture("returns15_synapse4_conformance_ir");
    assert_eq!(ir.ir_version, "3.0");
    assert_eq!(method(&ir, "conformance.load").md_cardinality, Cardinality::Many);
    assert_eq!(named_local(&method(&ir, "conformance.lookup").md_error).as_deref(), Some("LookupError"));
    match &ir.ir_types["conformance.SchemaResult"].td_kind {
        TypeKind::KindEnum { ke_repr, .. } => assert_eq!(*ke_repr, EnumRepr::Untagged),
        k => panic!("{k:?}"),
    }
    let files = ts(&ir);
    assert!(files["conformance/types.ts"].contains("export type SchemaResult = SchemaResultPluginSchema | SchemaResultMethodSchema;"));
    assert!(files["conformance/client.ts"].contains("  schema(): Promise<SchemaResult>;"));
    type_checks(&files);
    cargo_checks(&ir, "conformance4");
}

// ─────────────────────────────────────────────────────────────
// 2. IR 2.x reads as it did
// ─────────────────────────────────────────────────────────────

fn one_method_ir(version: &str, method: serde_json::Value) -> IR {
    serde_json::from_value(serde_json::json!({
        "irVersion": version,
        "irBackend": "kiln",
        "irTypes": {},
        "irMethods": { "kiln.fire": method },
        "irPlugins": { "kiln": ["fire"] }
    }))
    .expect("decodes")
}

fn fire(extra: serde_json::Value) -> serde_json::Value {
    let mut m = serde_json::json!({
        "mdName": "fire", "mdFullPath": "kiln.fire", "mdNamespace": "kiln",
        "mdDescription": "Fire the kiln.",
        "mdParams": [{ "pdName": "cone", "pdType": { "tag": "RefPrimitive", "contents": ["integer", null] }, "pdRequired": true }],
        "mdReturns": { "tag": "RefPrimitive", "contents": ["string", null] }
    });
    m.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    m
}

#[test]
fn ir2_streaming_is_the_cardinality() {
    let many = one_method_ir("2.0", fire(serde_json::json!({ "mdStreaming": true })));
    assert_eq!(method(&many, "kiln.fire").md_cardinality, Cardinality::Many);
    let one = one_method_ir("2.0", fire(serde_json::json!({ "mdStreaming": false })));
    assert_eq!(method(&one, "kiln.fire").md_cardinality, Cardinality::One);
    let absent = one_method_ir("2.0", fire(serde_json::json!({})));
    assert_eq!(method(&absent, "kiln.fire").md_cardinality, Cardinality::One);
    for ir in [&many, &one, &absent] {
        let m = method(ir, "kiln.fire");
        assert!(m.md_error.is_none() && m.md_progress.is_none());
    }
}

/// RETURNS-36 review #6: IR 3.0 always says a method's cardinality; one that
/// does not is refused, naming the method — never guessed `One`. IR 2.x
/// keeps deriving it from `mdStreaming` (absent: `One`).
#[test]
fn ir3_without_cardinality_is_refused_naming_the_method() {
    let json = |version: &str| serde_json::json!({
        "irVersion": version,
        "irBackend": "kiln",
        "irTypes": {},
        "irMethods": { "kiln.fire": fire(serde_json::json!({ "mdStreaming": true })) },
        "irPlugins": { "kiln": ["fire"] }
    });
    let err = serde_json::from_value::<IR>(json("3.0")).expect_err("an IR 3.0 method with no mdCardinality").to_string();
    assert!(err.contains("kiln.fire") && err.contains("mdCardinality"), "{err}");
    // with it, the same method reads
    let mut ok = json("3.0");
    ok["irMethods"]["kiln.fire"]["mdCardinality"] = serde_json::json!("many");
    let ir: IR = serde_json::from_value(ok).expect("decodes");
    assert_eq!(method(&ir, "kiln.fire").md_cardinality, Cardinality::Many);
    // IR 2.x: derived, as before
    for v in ["2.0", "2.1"] {
        let ir: IR = serde_json::from_value(json(v)).unwrap_or_else(|e| panic!("{v}: {e}"));
        assert_eq!(method(&ir, "kiln.fire").md_cardinality, Cardinality::Many, "{v}");
    }
}

#[test]
fn a_method_generates_the_same_from_ir2_and_ir3() {
    for (streaming, card) in [(true, "many"), (false, "one")] {
        let two = one_method_ir("2.0", fire(serde_json::json!({ "mdStreaming": streaming })));
        let three = one_method_ir("3.0", fire(serde_json::json!({ "mdCardinality": card, "mdError": null, "mdProgress": null })));
        // the metadata names the IR's version, and the package's version is
        // a hash of the files; every other file is the same
        let strip = |mut m: HashMap<String, String>| {
            m.remove(".codegen-metadata.json");
            m.remove("package.json");
            m
        };
        let (a, b) = (strip(ts(&two)), strip(ts(&three)));
        assert_eq!(a, b, "streaming {streaming}");
        assert!(a["kiln/client.ts"].contains(if streaming { "MethodStream<'kiln.fire', string, Up<A, KilnEvent>>" } else { "Promise<string>" }));
        let (a, b) = (generate_rust(&two).unwrap().files, generate_rust(&three).unwrap().files);
        assert_eq!(a["src/kiln/mod.rs"], b["src/kiln/mod.rs"], "streaming {streaming}");
    }
}

#[test]
fn a_method_writes_its_cardinality() {
    let ir = fixture("returns15_conformance_ir");
    let v = serde_json::to_value(method(&ir, "conformance.load")).unwrap();
    assert_eq!(v["mdCardinality"], "many");
    assert!(v.get("mdStreaming").is_none(), "{v}");
    assert!(v.get("mdError").is_some() && v.get("mdProgress").is_some(), "{v}");
}

// ─────────────────────────────────────────────────────────────
// 3. The versions read
// ─────────────────────────────────────────────────────────────

#[test]
fn only_ir_2_0_through_3_1_are_read() {
    // 3.1: 3.0 without the bidir keys (ASK-1).
    for v in ["2.0", "2.1", "3.0", "3.1"] {
        check_ir_version(v).unwrap_or_else(|e| panic!("{v}: {e}"));
        let ir = one_method_ir(v, fire(serde_json::json!({ "mdCardinality": "one" })));
        generate_typescript(&ir, &GenerationOptions::default()).unwrap_or_else(|e| panic!("{v}: {e}"));
        generate_rust(&ir).unwrap_or_else(|e| panic!("{v}: {e}"));
    }
    for v in ["1.0", "2.2", "3.2", "4.0", ""] {
        let msg = check_ir_version(v).expect_err(v).to_string();
        assert!(msg.contains(&format!("IR {v} is not supported")), "{msg}");
        assert!(msg.contains("reads IR 2.0, 2.1, 3.0 and 3.1"), "{msg}");
        let ir = one_method_ir(v, fire(serde_json::json!({})));
        assert_eq!(generate_typescript(&ir, &GenerationOptions::default()).err().map(|e| e.to_string()), Some(msg.clone()));
        assert_eq!(generate_rust(&ir).err().map(|e| e.to_string()), Some(msg));
    }
}

// ─────────────────────────────────────────────────────────────
// 4. IR 2.1's forms
// ─────────────────────────────────────────────────────────────

#[test]
fn ir21_refs_decode() {
    let r = |v: serde_json::Value| -> TypeRef { serde_json::from_value(v).unwrap() };
    let s = TypeRef::RefPrimitive("string".into(), None);
    assert_eq!(r(serde_json::json!({ "tag": "RefJson" })), TypeRef::RefAny);
    assert_eq!(r(serde_json::json!({ "tag": "RefAny" })), TypeRef::RefAny);
    assert_eq!(r(serde_json::json!({ "tag": "RefChild", "contents": "tickets.inputs" })), TypeRef::RefAny);
    assert_eq!(
        r(serde_json::json!({ "tag": "RefRefined", "contents": [{ "tag": "RefPrimitive", "contents": ["string", null] }, { "maxLength": 3 }] })),
        s
    );
    assert_eq!(
        r(serde_json::json!({ "tag": "RefMap", "contents": { "tag": "RefPrimitive", "contents": ["string", null] } })),
        TypeRef::RefMap(Box::new(s.clone()))
    );
    assert_eq!(
        r(serde_json::json!({ "tag": "RefTuple", "contents": [{ "tag": "RefPrimitive", "contents": ["string", null] }, { "tag": "RefPrimitive", "contents": ["integer", "int64"] }] })),
        TypeRef::RefTuple(vec![s, TypeRef::RefPrimitive("integer".into(), Some("int64".into()))])
    );
}

#[test]
fn ir21_forms_generate() {
    let ir = fixture("returns15_forms_ir");
    match &ir.ir_types["kiln.Source"].td_kind {
        TypeKind::KindEnum { ke_repr, .. } => assert_eq!(*ke_repr, EnumRepr::External),
        k => panic!("{k:?}"),
    }
    // mdInjected is a param with its source: documented, never sent
    let source = method(&ir, "kiln.source");
    assert_eq!(source.md_params.len(), 2);
    assert_eq!(source.sent_params().iter().map(|p| p.pd_name.as_str()).collect::<Vec<_>>(), ["id"]);
    assert_eq!(source.injected_breadcrumbs(), ["@requiresAuth — resolver: self.access.verify"]);

    let files = ts(&ir);
    let types = &files["kiln/types.ts"];
    for line in [
        "  temps: Record<string, number>;",
        "  window: [number, number];",
        "  label: string;",
        "  extra?: unknown;",
        "  shelves: Record<string, Shelf>[];",
        "export type SourceBrowser = { browser: { redirect: string } };",
        "export type SourceAgent = 'agent';",
        "export type SourceKey = { key: KeyRef };",
        "export type Source = SourceBrowser | SourceAgent | SourceKey;",
        "export type AnswerFiring = Firing;",
        "export type Answer = AnswerFiring | AnswerKeyRef;",
        "  kind: 'not_found';",
    ] {
        assert!(types.contains(line), "{line}:\n{types}");
    }
    let client = &files["kiln/client.ts"];
    assert!(client.contains("  source(id: string): Promise<Source>;"), "{client}");
    assert!(client.contains("@requiresAuth — resolver: self.access.verify"), "{client}");
    assert!(client.contains("  temps(): Promise<Record<string, number>>;"), "{client}");
    assert!(client.contains("  watch(since?: number | null): MethodStream<'kiln.watch', Answer, Up<A, KilnEvent>>;"), "{client}");

    let rust = generate_rust(&ir).expect("rust").files;
    let module = &rust["src/kiln/mod.rs"];
    for line in [
        "    pub temps: std::collections::BTreeMap<String, f64>,",
        "    pub window: (i64, i64),",
        "#[serde(untagged)]\npub enum Answer {",
        "    Firing(Firing),",
        "#[derive(Debug, Clone, Serialize, Deserialize)]\npub enum Source {",
        "    Key(KeyRef),",
        "#[serde(tag = \"kind\")]\npub enum KilnError {",
        "pub async fn source(client: &PlexusClient, id: String) -> Result<Source> {",
    ] {
        assert!(module.contains(line), "{line}:\n{module}");
    }
}

#[test]
fn ir21_forms_typescript_type_checks() {
    type_checks(&ts(&fixture("returns15_forms_ir")));
}

#[test]
fn ir21_forms_rust_compiles() {
    cargo_checks(&fixture("returns15_forms_ir"), "kiln");
}
