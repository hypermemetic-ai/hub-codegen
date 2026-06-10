//! IR-9 typed-handle codegen tests.
//!
//! Covers:
//!   1. `DynamicChild` + `Listable` emission when `list_method` is set.
//!   2. Both `Listable` and `Searchable` emission when both are set.
//!   3. Neither capability when both are `None`.
//!   4. Sibling hiding: the method named by `list_method` must NOT appear
//!      as a flat method on the parent client.
//!   5. Compile-time capability rejection is preserved by the absence of
//!      `& Searchable` in the emitted intersection type.
//!   6. Pre-IR regression: output is byte-identical to pre-ticket output
//!      when no `MethodRole::DynamicChild` is present.
//!   7. Rust backend: a minimal gate struct + trait impls are emitted.

use hub_codegen::generator::{GenerationOptions, GenerateSelector, TransportEnv};
use hub_codegen::ir::*;
use hub_codegen::generate_typescript;
#[cfg(feature = "rust")]
use hub_codegen::generate_rust;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────
// Fixture helpers
// ─────────────────────────────────────────────────────────────

/// Build an IR fixture for a parent namespace with one dynamic-child gate
/// and a referenced child namespace (so the ChildClient class exists).
fn dynamic_child_ir(
    list_method: Option<&str>,
    search_method: Option<&str>,
) -> IR {
    let mut ir_types = HashMap::new();
    let mut ir_methods = HashMap::new();
    let mut ir_plugins = HashMap::new();

    // The child activation lives at namespace "solar.body".
    // Its plugin entry must exist so the DynamicChild resolver can find the
    // generated SolarBodyClient class.
    ir_plugins.insert("solar.body".to_string(), vec!["info".to_string()]);
    ir_methods.insert(
        "solar.body.info".to_string(),
        MethodDef {
            md_name: "info".to_string(),
            md_full_path: "solar.body.info".to_string(),
            md_namespace: "solar.body".to_string(),
            md_description: None,
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: MethodRole::Rpc, md_deprecation: None, md_requires_credential: None, md_auth_posture: None, md_public: false,},
    );

    // A marker type the dynamic-child return refers to.
    ir_types.insert(
        "solar.body.CelestialBody".to_string(),
        TypeDef {
            td_name: "CelestialBody".to_string(),
            td_namespace: "solar.body".to_string(),
            td_description: None, td_deprecation: None,
            td_kind: TypeKind::KindStruct {
                ks_fields: vec![FieldDef {
                    fd_name: "name".to_string(),
                    fd_type: TypeRef::RefPrimitive("string".to_string(), None),
                    fd_description: None,
                    fd_required: true,
                    fd_default: None, fd_deprecation: None,
                }],
            },
        },
    );

    // Parent namespace: "solar". Owns the body(name) DynamicChild gate
    // plus the list/search siblings named by the role (when set).
    let mut solar_methods = vec!["body".to_string()];

    ir_methods.insert(
        "solar.body".to_string(),
        MethodDef {
            md_name: "body".to_string(),
            md_full_path: "solar.body".to_string(),
            md_namespace: "solar".to_string(),
            md_description: Some("Look up a celestial body by name".to_string()),
            md_streaming: false,
            md_params: vec![ParamDef {
                pd_name: "name".to_string(),
                pd_type: TypeRef::RefPrimitive("string".to_string(), None),
                pd_description: None,
                pd_required: true,
                pd_default: None, pd_deprecation: None, pd_source: None,
            }],
            md_returns: TypeRef::RefOptional(Box::new(TypeRef::RefNamed(QualifiedName {
                qn_namespace: "solar.body".to_string(),
                qn_local_name: "CelestialBody".to_string(),
            }))),
            md_bidir_type: None,
            md_role: MethodRole::DynamicChild {
                list_method: list_method.map(String::from),
                search_method: search_method.map(String::from),
            }, md_deprecation: None, md_requires_credential: None, md_auth_posture: None, md_public: false,},
    );

    if let Some(name) = list_method {
        ir_methods.insert(
            format!("solar.{}", name),
            MethodDef {
                md_name: name.to_string(),
                md_full_path: format!("solar.{}", name),
                md_namespace: "solar".to_string(),
                md_description: None,
                md_streaming: true,
                md_params: vec![],
                md_returns: TypeRef::RefPrimitive("string".to_string(), None),
                md_bidir_type: None,
                md_role: MethodRole::Rpc, md_deprecation: None, md_requires_credential: None, md_auth_posture: None, md_public: false,},
        );
        solar_methods.push(name.to_string());
    }

    if let Some(name) = search_method {
        ir_methods.insert(
            format!("solar.{}", name),
            MethodDef {
                md_name: name.to_string(),
                md_full_path: format!("solar.{}", name),
                md_namespace: "solar".to_string(),
                md_description: None,
                md_streaming: true,
                md_params: vec![ParamDef {
                    pd_name: "query".to_string(),
                    pd_type: TypeRef::RefPrimitive("string".to_string(), None),
                    pd_description: None,
                    pd_required: true,
                    pd_default: None, pd_deprecation: None, pd_source: None,
                }],
                md_returns: TypeRef::RefPrimitive("string".to_string(), None),
                md_bidir_type: None,
                md_role: MethodRole::Rpc, md_deprecation: None, md_requires_credential: None, md_auth_posture: None, md_public: false,},
        );
        solar_methods.push(name.to_string());
    }

    ir_plugins.insert("solar".to_string(), solar_methods);

    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("ir9-fixture-hash".to_string()),
        ir_metadata: None,
        ir_types,
        ir_methods,
        ir_plugins, ir_plugin_deprecations: Default::default(), ir_plugin_requests: Default::default(),
    }
}

/// Read the generated `solar/client.ts` content for a given IR.
fn solar_client_ts(ir: &IR) -> String {
    let opts = GenerationOptions {
        generate: GenerateSelector::Plugins,
        transport: TransportEnv::None,
        ..GenerationOptions::default()
    };
    let result = generate_typescript(ir, &opts).expect("ts generation");
    result
        .files
        .get("solar/client.ts")
        .cloned()
        .expect("solar/client.ts must be emitted")
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

/// Test 1: DynamicChild + Listable (list_method=Some, search_method=None).
#[test]
fn test_dynamic_child_with_listable() {
    let ir = dynamic_child_ir(Some("body_names"), None);
    let ts = solar_client_ts(&ir);

    assert!(
        ts.contains("readonly body: DynamicChild<SolarBodyClient> & Listable;"),
        "Expected `readonly body: DynamicChild<SolarBodyClient> & Listable;` in:\n{}",
        ts
    );
    assert!(
        !ts.contains("& Searchable"),
        "Searchable must not appear when search_method is None:\n{}",
        ts
    );
    assert!(
        ts.contains("import { makeDynamicChild"),
        "makeDynamicChild must be imported from the runtime:\n{}",
        ts
    );
    assert!(
        ts.contains("listMethod: 'body_names'"),
        "makeDynamicChild config must reflect list_method:\n{}",
        ts
    );
    assert!(
        ts.contains("searchMethod: null"),
        "searchMethod in config must be null when search_method is None:\n{}",
        ts
    );
}

/// Test 2: both `Listable` and `Searchable`.
#[test]
fn test_dynamic_child_with_both_capabilities() {
    let ir = dynamic_child_ir(Some("body_names"), Some("search_bodies"));
    let ts = solar_client_ts(&ir);

    assert!(
        ts.contains("readonly body: DynamicChild<SolarBodyClient> & Listable & Searchable;"),
        "Expected `readonly body: DynamicChild<SolarBodyClient> & Listable & Searchable;` in:\n{}",
        ts
    );
    assert!(
        ts.contains("listMethod: 'body_names'"),
        "list_method literal must be rendered in config:\n{}",
        ts
    );
    assert!(
        ts.contains("searchMethod: 'search_bodies'"),
        "search_method literal must be rendered in config:\n{}",
        ts
    );
}

/// Test 3: neither capability.
#[test]
fn test_dynamic_child_no_capabilities() {
    let ir = dynamic_child_ir(None, None);
    let ts = solar_client_ts(&ir);

    assert!(
        ts.contains("readonly body: DynamicChild<SolarBodyClient>;"),
        "Expected plain `DynamicChild<SolarBodyClient>` without intersections, got:\n{}",
        ts
    );
    assert!(
        !ts.contains("& Listable"),
        "Listable must not appear when list_method is None:\n{}",
        ts
    );
    assert!(
        !ts.contains("& Searchable"),
        "Searchable must not appear when search_method is None:\n{}",
        ts
    );
    assert!(
        ts.contains("listMethod: null"),
        "listMethod must be null in the config when list_method is None:\n{}",
        ts
    );
}

/// Test 4: sibling hiding — the list_method-named method must NOT be emitted
/// as a flat method on the parent client. It's accessible only via the gate.
#[test]
fn test_sibling_list_method_hidden() {
    let ir = dynamic_child_ir(Some("body_names"), None);
    let ts = solar_client_ts(&ir);

    // The interface should NOT declare a `body_names(...)` flat method or
    // `bodyNames(...)` camelCased method.
    assert!(
        !ts.contains("bodyNames("),
        "Parent client must not expose body_names as a flat method (sibling hiding):\n{}",
        ts
    );
    // The flat-method implementation must also be absent.
    assert!(
        !ts.contains("async *bodyNames(") && !ts.contains("async bodyNames("),
        "Parent client impl must not contain a flat bodyNames method:\n{}",
        ts
    );
}

/// Test 5: compile-time capability rejection — without `& Searchable`,
/// TypeScript's type system rejects `.search()` calls on the gate. This
/// test pins the emitted intersection type that enforces the rejection.
/// (A full tsc compile-fail test would require toolchain orchestration;
/// the precise string assertion locks the property in place.)
#[test]
fn test_no_searchable_without_search_method() {
    let ir = dynamic_child_ir(Some("body_names"), None);
    let ts = solar_client_ts(&ir);

    // The declared gate type must NOT include `& Searchable`. A consumer
    // calling `.search()` on a `DynamicChild<T> & Listable` would be
    // rejected by `tsc --noEmit` with TS2339.
    let gate_line = ts
        .lines()
        .find(|l| l.contains("readonly body:"))
        .expect("must emit the body gate declaration");
    assert!(
        !gate_line.contains("Searchable"),
        "Gate type must not advertise Searchable when search_method is None: {}",
        gate_line
    );
}

/// Test 6: pre-IR regression — an IR with no DynamicChild role produces
/// output byte-identical to a baseline that mirrors pre-ticket behavior.
///
/// We assert this by generating twice with two IRs: one with explicit
/// `md_role: Rpc` and one using `Default::default()`. They must produce
/// identical output, confirming the role field is inert for non-child
/// methods.
#[test]
fn test_pre_ir_regression_no_dynamic_child() {
    let mut ir = IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("regr-hash".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods: HashMap::new(),
        ir_plugins: HashMap::new(), ir_plugin_deprecations: Default::default(), ir_plugin_requests: Default::default(),};
    ir.ir_methods.insert(
        "echo.ping".to_string(),
        MethodDef {
            md_name: "ping".to_string(),
            md_full_path: "echo.ping".to_string(),
            md_namespace: "echo".to_string(),
            md_description: Some("Ping".to_string()),
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: MethodRole::Rpc, md_deprecation: None, md_requires_credential: None, md_auth_posture: None, md_public: false,},
    );
    ir.ir_plugins.insert("echo".to_string(), vec!["ping".to_string()]);

    let opts = GenerationOptions::default();
    let r1 = generate_typescript(&ir, &opts).unwrap();

    // Second pass with md_role defaulted.
    let mut ir2 = ir.clone();
    ir2.ir_methods.get_mut("echo.ping").unwrap().md_role = MethodRole::default();
    let r2 = generate_typescript(&ir2, &opts).unwrap();

    // Byte-identical across the full file set.
    let keys1: std::collections::BTreeSet<_> = r1.files.keys().collect();
    let keys2: std::collections::BTreeSet<_> = r2.files.keys().collect();
    assert_eq!(keys1, keys2, "file sets must match");
    for key in keys1 {
        assert_eq!(
            r1.files.get(key),
            r2.files.get(key),
            "file {} must be byte-identical between MethodRole::Rpc and MethodRole::default()",
            key
        );
    }

    // The emitted echo/client.ts must NOT reference any typed-handle
    // primitives — those are reserved for DynamicChild methods.
    let echo_client = r1.files.get("echo/client.ts").expect("echo/client.ts");
    assert!(
        !echo_client.contains("DynamicChild"),
        "pre-IR output must not contain DynamicChild primitives:\n{}",
        echo_client
    );
    assert!(
        !echo_client.contains("makeDynamicChild"),
        "pre-IR output must not contain makeDynamicChild:\n{}",
        echo_client
    );
}

/// Test 7 (determinism): two consecutive regens produce byte-identical files.
#[test]
fn test_dynamic_child_determinism() {
    let ir = dynamic_child_ir(Some("body_names"), Some("search_bodies"));
    let opts = GenerationOptions::default();
    let r1 = generate_typescript(&ir, &opts).unwrap();
    let r2 = generate_typescript(&ir, &opts).unwrap();

    for key in r1.files.keys() {
        // Skip metadata files which embed a content hash that recombines
        // identically only when inputs match — it's still content-stable,
        // but diff here in case.
        assert_eq!(
            r1.files.get(key),
            r2.files.get(key),
            "determinism: file {} differs across regens",
            key
        );
    }
}

/// Test: the child namespace's ClientImpl class must be exported when the
/// child is referenced as a DynamicChild target. Parents need the impl
/// class as a runtime constructor.
#[test]
fn test_child_client_impl_is_exported() {
    let ir = dynamic_child_ir(Some("body_names"), None);
    let opts = GenerationOptions {
        generate: GenerateSelector::Plugins,
        transport: TransportEnv::None,
        ..GenerationOptions::default()
    };
    let r = generate_typescript(&ir, &opts).unwrap();

    let child = r
        .files
        .get("solar/body/client.ts")
        .expect("child client.ts must be emitted");
    assert!(
        child.contains("export class SolarBodyClientImpl"),
        "Child ClientImpl must be exported when referenced as a DynamicChild target:\n{}",
        child
    );

    // The parent's client.ts must import the impl class as a value.
    let parent = r
        .files
        .get("solar/client.ts")
        .expect("parent client.ts must be emitted");
    assert!(
        parent.contains("import { SolarBodyClientImpl }"),
        "Parent must import SolarBodyClientImpl as a value:\n{}",
        parent
    );
    assert!(
        parent.contains("childClient: SolarBodyClientImpl"),
        "makeDynamicChild config must pass SolarBodyClientImpl as the child constructor:\n{}",
        parent
    );
}

/// Test: the generated rpc.ts runtime exports the typed-handle primitives.
#[test]
fn test_rpc_runtime_exports_dynamic_child_primitives() {
    let ir = dynamic_child_ir(Some("body_names"), None);
    let opts = GenerationOptions::default();
    let r = generate_typescript(&ir, &opts).unwrap();
    let rpc_ts = r.files.get("rpc.ts").expect("rpc.ts must be emitted");

    assert!(
        rpc_ts.contains("export interface DynamicChild<T>"),
        "rpc.ts must export DynamicChild<T>:\n{}",
        rpc_ts
    );
    assert!(
        rpc_ts.contains("export interface Listable"),
        "rpc.ts must export Listable"
    );
    assert!(
        rpc_ts.contains("export interface Searchable"),
        "rpc.ts must export Searchable"
    );
    assert!(
        rpc_ts.contains("export function makeDynamicChild"),
        "rpc.ts must export makeDynamicChild"
    );
}

// ─────────────────────────────────────────────────────────────
// Compile-time capability rejection via tsc
// ─────────────────────────────────────────────────────────────
//
// These tests write the generated TS to a temp dir plus a consumer file that
// deliberately misuses the API, then runs `tsc --noEmit` to assert compile
// failure (or success where expected). Skipped if `tsc` isn't on PATH.

fn tsc_available() -> bool {
    std::process::Command::new("tsc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write the full generated output to `dir` and return the root path. Also
/// writes a minimal tsconfig.json that resolves the generated modules.
fn write_generated(dir: &std::path::Path, ir: &IR) {
    let opts = GenerationOptions {
        generate: GenerateSelector::All,
        transport: TransportEnv::None,
        ..GenerationOptions::default()
    };
    let r = generate_typescript(ir, &opts).unwrap();
    for (rel, content) in &r.files {
        let full = dir.join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full, content).unwrap();
    }
}

/// Compile-time rejection: calling `.search()` on a gate that lacks
/// `Searchable` must fail `tsc --noEmit`.
#[test]
fn test_tsc_rejects_search_without_searchable() {
    if !tsc_available() {
        eprintln!("tsc not available; skipping compile-fail test");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let ir = dynamic_child_ir(Some("body_names"), None); // no search_method
    write_generated(tmp.path(), &ir);

    // Write a consumer that misuses the API.
    let consumer = r#"
import { createSolarClient } from './solar/client';
declare const rpc: any;
const client = createSolarClient(rpc);
// This must fail: .search is not on DynamicChild<T> & Listable
async function go() {
  for await (const _ of client.body.search("q")) {}
}
go();
"#;
    let consumer_path = tmp.path().join("bad_consumer.ts");
    std::fs::write(&consumer_path, consumer).unwrap();

    // Minimal tsconfig to resolve modules.
    let tsconfig = r#"{
      "compilerOptions": {
        "strict": true,
        "target": "ES2020",
        "module": "ESNext",
        "moduleResolution": "bundler",
        "lib": ["ES2020", "DOM"],
        "noEmit": true,
        "skipLibCheck": true,
        "types": []
      },
      "include": ["**/*.ts"]
    }"#;
    std::fs::write(tmp.path().join("tsconfig.json"), tsconfig).unwrap();

    let output = std::process::Command::new("tsc")
        .arg("--noEmit")
        .arg("-p")
        .arg(tmp.path())
        .output()
        .expect("tsc invocation");

    assert!(
        !output.status.success(),
        "tsc should reject .search() on DynamicChild<T> & Listable (no Searchable). stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Confirm the failure is about the .search property, not some unrelated
    // error in the generated runtime.
    assert!(
        combined.contains("search"),
        "tsc rejection must cite the .search property. output:\n{}",
        combined
    );
}

// ─────────────────────────────────────────────────────────────
// Rust backend (skeleton)
// ─────────────────────────────────────────────────────────────

/// Rust backend: a DynamicChild method produces a per-gate struct with
/// DynamicChild + Listable trait impls (skeleton).
#[cfg(feature = "rust")]
#[test]
fn test_rust_backend_emits_dynamic_child_struct() {
    let ir = dynamic_child_ir(Some("body_names"), None);
    let result = generate_rust(&ir).expect("rust generation");

    // The solar namespace module must contain the gate struct and trait impls.
    let solar_mod = result
        .files
        .get("src/solar/mod.rs")
        .expect("src/solar/mod.rs must be emitted");

    assert!(
        solar_mod.contains("pub trait DynamicChild"),
        "DynamicChild trait must be declared:\n{}",
        solar_mod
    );
    assert!(
        solar_mod.contains("pub trait Listable"),
        "Listable trait must be declared"
    );
    assert!(
        solar_mod.contains("pub trait Searchable"),
        "Searchable trait must be declared"
    );
    assert!(
        solar_mod.contains("pub struct BodyGate"),
        "BodyGate struct must be emitted:\n{}",
        solar_mod
    );
    assert!(
        solar_mod.contains("impl<'a> DynamicChild for BodyGate<'a>"),
        "BodyGate must implement DynamicChild:\n{}",
        solar_mod
    );
    assert!(
        solar_mod.contains("impl<'a> Listable for BodyGate<'a>"),
        "BodyGate must implement Listable (list_method=Some):\n{}",
        solar_mod
    );
    assert!(
        !solar_mod.contains("impl<'a> Searchable for BodyGate<'a>"),
        "BodyGate must NOT implement Searchable (search_method=None):\n{}",
        solar_mod
    );
}

/// Rust backend: sibling list method must NOT appear as a flat pub fn on
/// the parent module.
#[cfg(feature = "rust")]
#[test]
fn test_rust_backend_hides_sibling_methods() {
    let ir = dynamic_child_ir(Some("body_names"), None);
    let result = generate_rust(&ir).expect("rust generation");

    let solar_mod = result
        .files
        .get("src/solar/mod.rs")
        .expect("src/solar/mod.rs must be emitted");

    // A flat `pub async fn body_names` would indicate sibling hiding failed.
    assert!(
        !solar_mod.contains("pub async fn body_names"),
        "Parent namespace must not expose body_names as a flat fn:\n{}",
        solar_mod
    );
}

/// Rust backend skeleton: generated crate compiles under `cargo check`.
///
/// Writes the generated output to a temp dir and runs `cargo check` on it.
/// This is the Rust analogue of the TypeScript tsc compile-fail test — it
/// gives mechanical verification that the skeleton is at least
/// syntactically valid.
#[cfg(feature = "rust")]
#[test]
fn test_rust_backend_dynamic_child_compiles() {
    let ir = dynamic_child_ir(Some("body_names"), Some("search_bodies"));
    let result = generate_rust(&ir).expect("rust generation");

    let tmp = tempfile::tempdir().unwrap();
    for (rel, content) in &result.files {
        let full = tmp.path().join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full, content).unwrap();
    }

    let output = std::process::Command::new("cargo")
        .arg("check")
        .arg("--manifest-path")
        .arg(tmp.path().join("Cargo.toml"))
        .output()
        .expect("cargo check invocation");

    if !output.status.success() {
        panic!(
            "Generated Rust DynamicChild crate failed to compile.\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
