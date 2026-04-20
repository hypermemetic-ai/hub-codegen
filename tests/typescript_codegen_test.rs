//! TypeScript codegen invariant tests.
//!
//! Focused on invariants that matter for real-world correctness:
//!   1. Transport dispatch — none/browser/ws emit the right artifacts
//!   2. Generate selector — each selector produces only the expected file set
//!   3. Three-way merge — user edits are preserved (Skip) or overwritten (Force)

use hub_codegen::cache::{CodeCacheManifest, CodePluginCache, ToolchainVersions};
use hub_codegen::generator::{GenerationOptions, GenerateSelector, TransportEnv};

use hub_codegen::hash::compute_file_hash;
use hub_codegen::ir::*;
use hub_codegen::merge::{merge_generated_code, MergeStrategy};
use hub_codegen::generate_typescript;
use std::collections::HashMap;
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────
// Minimal IR fixture
// ─────────────────────────────────────────────────────────────

fn minimal_ir() -> IR {
    let mut ir_methods = HashMap::new();
    ir_methods.insert(
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
            md_role: Default::default(), md_deprecation: None,},
    );
    let mut ir_plugins = HashMap::new();
    ir_plugins.insert("echo".to_string(), vec!["ping".to_string()]);
    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("test-hash-abc123".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods,
        ir_plugins, ir_plugin_deprecations: Default::default(),
    }
}

/// Build a CodeCacheManifest that records the hashes from a GenerationResult,
/// simulating what main.rs writes to disk after a successful first run.
fn cache_from_result(
    file_hashes: &HashMap<String, String>,
) -> CodeCacheManifest {
    let mut plugins = HashMap::new();
    plugins.insert(
        "echo".to_string(),
        CodePluginCache {
            ir_hash: "test-hash-abc123".to_string(),
            file_hashes: file_hashes.clone(),
            cached_at: "2026-01-01T00:00:00Z".to_string(),
        },
    );
    CodeCacheManifest {
        version: "2.0".to_string(),
        target: "typescript".to_string(),
        toolchain: ToolchainVersions {
            synapse_cc: "0.2.0".to_string(),
            synapse: "0.0.1".to_string(),
            hub_codegen: "0.2.0".to_string(),
        },
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        plugins,
    }
}

// ─────────────────────────────────────────────────────────────
// Transport dispatch
// ─────────────────────────────────────────────────────────────

/// `--transport none`: monorepo mode — no transport.ts emitted.
/// The consumer provides their own @plexus/rpc-client as a workspace dep;
/// no external runtime deps should be injected.
/// Regression would silently break all monorepo consumers.
#[test]
fn test_transport_none_no_transport_file() {
    let ir = minimal_ir();
    let result = generate_typescript(&ir, &GenerationOptions { transport: TransportEnv::None, ..GenerationOptions::default() }).unwrap();

    assert!(
        !result.files.contains_key("transport.ts"),
        "transport none must not emit transport.ts"
    );
    assert!(
        result.dependencies.is_empty(),
        "transport none must not inject external runtime deps (consumer uses workspace:*): got {:?}",
        result.dependencies
    );
}

/// `--transport browser`: native WebSocket — no `ws` import so Tauri/WebView builds work.
#[test]
fn test_transport_browser_no_ws_import() {
    let ir = minimal_ir();
    let result = generate_typescript(&ir, &GenerationOptions { transport: TransportEnv::Browser, ..GenerationOptions::default() }).unwrap();

    let transport = result.files.get("transport.ts").expect("browser transport must emit transport.ts");
    assert!(
        !transport.contains("import WebSocket from 'ws'"),
        "browser transport must not import 'ws' package"
    );
    assert!(
        !result.dependencies.contains_key("ws"),
        "browser transport must not declare ws dep"
    );
}

/// `--transport ws` (default): Node.js mode — ws import and dep must be present.
#[test]
fn test_transport_ws_has_ws_import() {
    let ir = minimal_ir();
    let result = generate_typescript(&ir, &GenerationOptions { transport: TransportEnv::Ws, ..GenerationOptions::default() }).unwrap();

    let transport = result.files.get("transport.ts").expect("ws transport must emit transport.ts");
    assert!(
        transport.contains("import WebSocket from 'ws'"),
        "ws transport must import 'ws'"
    );
    assert!(
        result.dependencies.contains_key("ws"),
        "ws transport must declare ws dep"
    );
}

// ─────────────────────────────────────────────────────────────
// Generated artifact consistency
// ─────────────────────────────────────────────────────────────
//
// These tests ensure the generated artifacts are mutually compatible.
// The failure mode they guard against: test runner changed in package.json
// but smoke test still imports from the old module (or vice versa), causing
// silent breakage that only surfaces when a user runs the generated project.

/// The generated `test` script must invoke bun, not tsx/npx/node.
/// Regression: hub-codegen previously emitted `npx tsx test/smoke.test.ts`
/// while smoke tests already imported from `"bun:test"` — incompatible.
#[test]
fn test_package_json_test_script_uses_bun() {
    let ir = minimal_ir();
    let result = generate_typescript(&ir, &GenerationOptions { transport: TransportEnv::Ws, ..GenerationOptions::default() }).unwrap();

    let pkg = result.files.get("package.json").expect("package.json must be emitted");
    assert!(
        pkg.contains("\"test\": \"bun test\""),
        "package.json test script must use `bun test`, got:\n{}", pkg
    );
    assert!(
        !pkg.contains("tsx"),
        "package.json must not reference tsx (use bun instead)"
    );
    assert!(
        !pkg.contains("npx"),
        "package.json must not reference npx for tests"
    );
}

/// Smoke tests must import from `"bun:test"`, not from ts-node, jest, or vitest.
/// If this changes, `package.json` test script must change in lockstep.
#[test]
fn test_smoke_test_imports_from_bun_test() {
    let ir = minimal_ir();
    let result = generate_typescript(&ir, &GenerationOptions { transport: TransportEnv::Ws, ..GenerationOptions::default() }).unwrap();

    // Find the smoke test file(s)
    let smoke_files: Vec<(&String, &String)> = result.files.iter()
        .filter(|(k, _)| k.starts_with("test/") && k.ends_with(".ts"))
        .collect();
    assert!(!smoke_files.is_empty(), "at least one test/*.ts file must be generated");

    for (path, content) in &smoke_files {
        assert!(
            content.contains("from \"bun:test\""),
            "{path} must import from \"bun:test\", not from another test framework"
        );
        assert!(
            !content.contains("from 'jest'") && !content.contains("from \"jest\""),
            "{path} must not import from jest"
        );
    }
}

/// bun-types must be in dev deps.
/// The smoke tests import from "bun:test"; tsc can only resolve that module
/// when bun-types is installed. If bun-types is missing, tsc fails with
/// TS2307 even though the test runner (bun) works fine.
#[test]
fn test_bun_types_in_dev_deps() {
    use hub_codegen::generator::typescript::package::get_dev_deps;
    let dev_deps = get_dev_deps(TransportEnv::Ws);
    assert!(
        dev_deps.contains_key("bun-types"),
        "bun-types must be in dev deps so tsc can resolve bun:test imports: got {:?}", dev_deps
    );
}

// ─────────────────────────────────────────────────────────────
// Generate selector
// ─────────────────────────────────────────────────────────────
//
// Each selector must produce exactly the expected file set and nothing else.
// Regressions here would cause tendrils to silently produce wrong artifacts
// when synapse-cc invokes hub-codegen with targeted selectors.

/// `--generate transport`: types.ts + rpc.ts + transport.ts (modular, no IR needed).
#[test]
fn test_selector_transport_only() {
    let ir = minimal_ir();
    let opts = GenerationOptions { generate: GenerateSelector::Transport, ..GenerationOptions::default() };
    let result = generate_typescript(&ir, &opts).unwrap();

    assert_eq!(result.files.len(), 3, "GenTransport must emit exactly three files");
    assert!(result.files.contains_key("types.ts"),     "GenTransport must emit types.ts");
    assert!(result.files.contains_key("rpc.ts"),       "GenTransport must emit rpc.ts");
    assert!(result.files.contains_key("transport.ts"), "GenTransport must emit transport.ts");

    // transport.ts must import from ./types and ./rpc — not inline them
    let transport = result.files.get("transport.ts").unwrap();
    assert!(transport.contains("from './types'"), "transport.ts must import from ./types");
    assert!(transport.contains("from './rpc'"),   "transport.ts must import from ./rpc");

    // types.ts and rpc.ts must NOT contain PlexusRpcClient (that lives in transport)
    let types = result.files.get("types.ts").unwrap();
    assert!(!types.contains("PlexusRpcClient"), "types.ts must not contain PlexusRpcClient");
    let rpc = result.files.get("rpc.ts").unwrap();
    assert!(!rpc.contains("PlexusRpcClient"), "rpc.ts must not contain PlexusRpcClient");
}

/// `--generate transport` with `--transport none`: no transport file emitted.
#[test]
fn test_selector_transport_none_emits_nothing() {
    let ir = minimal_ir();
    let opts = GenerationOptions {
        generate: GenerateSelector::Transport,
        transport: TransportEnv::None,
        ..GenerationOptions::default()
    };
    let result = generate_typescript(&ir, &opts).unwrap();
    assert!(result.files.is_empty(), "GenTransport + TransportNone must emit no files");
}

/// `--generate rpc`: types.ts, rpc.ts, index.ts — no transport, no package.json.
#[test]
fn test_selector_rpc_only() {
    let ir = minimal_ir();
    let opts = GenerationOptions { generate: GenerateSelector::Rpc, ..GenerationOptions::default() };
    let result = generate_typescript(&ir, &opts).unwrap();

    assert!(result.files.contains_key("types.ts"), "GenRpc must emit types.ts");
    assert!(result.files.contains_key("rpc.ts"), "GenRpc must emit rpc.ts");
    assert!(result.files.contains_key("index.ts"), "GenRpc must emit index.ts");
    assert!(!result.files.contains_key("transport.ts"), "GenRpc must not emit transport.ts");
    assert!(!result.files.contains_key("package.json"), "GenRpc must not emit package.json");
}

/// `--generate plugins`: namespace client files only, no rpc.ts or transport.ts.
#[test]
fn test_selector_plugins_only() {
    let ir = minimal_ir();
    let opts = GenerationOptions { generate: GenerateSelector::Plugins, ..GenerationOptions::default() };
    let result = generate_typescript(&ir, &opts).unwrap();

    // minimal_ir has "echo" namespace — expect echo/ files
    assert!(
        result.files.keys().any(|k| k.starts_with("echo/")),
        "GenPlugins must emit namespace files; got: {:?}", result.files.keys().collect::<Vec<_>>()
    );
    assert!(!result.files.contains_key("rpc.ts"), "GenPlugins must not emit rpc.ts");
    assert!(!result.files.contains_key("transport.ts"), "GenPlugins must not emit transport.ts");
    assert!(!result.files.contains_key("package.json"), "GenPlugins must not emit package.json");
}

/// `--generate plugins --plugins nonexistent`: plugin filter produces no files.
#[test]
fn test_selector_plugins_filter_empty() {
    let ir = minimal_ir();
    let opts = GenerationOptions {
        generate: GenerateSelector::Plugins,
        plugins_filter: Some(vec!["nonexistent".to_string()]),
        ..GenerationOptions::default()
    };
    let result = generate_typescript(&ir, &opts).unwrap();
    assert!(
        result.files.is_empty(),
        "GenPlugins with non-matching filter must emit no files; got: {:?}",
        result.files.keys().collect::<Vec<_>>()
    );
}

// ─────────────────────────────────────────────────────────────
// Multi-namespace IR fixture (for scoped-generation tests)
// ─────────────────────────────────────────────────────────────

/// IR with three namespaces:
///   echo            — echo.ping
///   health          — health.status
///   solar.earth     — solar.earth.info
fn multi_ns_ir() -> IR {
    let mut ir_methods = HashMap::new();
    ir_methods.insert(
        "echo.ping".to_string(),
        MethodDef {
            md_name: "ping".to_string(),
            md_full_path: "echo.ping".to_string(),
            md_namespace: "echo".to_string(),
            md_description: None,
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: Default::default(), md_deprecation: None,},
    );
    ir_methods.insert(
        "health.status".to_string(),
        MethodDef {
            md_name: "status".to_string(),
            md_full_path: "health.status".to_string(),
            md_namespace: "health".to_string(),
            md_description: None,
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("boolean".to_string(), None),
            md_bidir_type: None,
            md_role: Default::default(), md_deprecation: None,},
    );
    ir_methods.insert(
        "solar.earth.info".to_string(),
        MethodDef {
            md_name: "info".to_string(),
            md_full_path: "solar.earth.info".to_string(),
            md_namespace: "solar.earth".to_string(),
            md_description: None,
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: Default::default(), md_deprecation: None,},
    );
    let mut ir_plugins = HashMap::new();
    ir_plugins.insert("echo".to_string(), vec!["ping".to_string()]);
    ir_plugins.insert("health".to_string(), vec!["status".to_string()]);
    ir_plugins.insert("solar.earth".to_string(), vec!["info".to_string()]);
    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("multi-hash-abc123".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods,
        ir_plugins, ir_plugin_deprecations: Default::default(),
    }
}

// ─────────────────────────────────────────────────────────────
// Namespace-scoped generation (plugins_filter)
// ─────────────────────────────────────────────────────────────

/// Exact-match filter: only the named namespace is generated.
#[test]
fn test_plugins_filter_exact_match() {
    let ir = multi_ns_ir();
    let opts = GenerationOptions {
        generate: GenerateSelector::Plugins,
        plugins_filter: Some(vec!["echo".to_string()]),
        ..GenerationOptions::default()
    };
    let result = generate_typescript(&ir, &opts).unwrap();

    assert!(
        result.files.keys().any(|k| k.starts_with("echo/")),
        "filter 'echo' must emit echo/ files"
    );
    assert!(
        !result.files.keys().any(|k| k.starts_with("health/")),
        "filter 'echo' must not emit health/ files; got: {:?}", result.files.keys().collect::<Vec<_>>()
    );
    assert!(
        !result.files.keys().any(|k| k.starts_with("solar/")),
        "filter 'echo' must not emit solar/ files"
    );
}

/// Prefix-match filter: a parent prefix matches child namespaces.
/// Filter "solar" must match "solar.earth" (dot-segment aware).
#[test]
fn test_plugins_filter_prefix_match() {
    let ir = multi_ns_ir();
    let opts = GenerationOptions {
        generate: GenerateSelector::Plugins,
        plugins_filter: Some(vec!["solar".to_string()]),
        ..GenerationOptions::default()
    };
    let result = generate_typescript(&ir, &opts).unwrap();

    assert!(
        result.files.keys().any(|k| k.starts_with("solar/")),
        "filter 'solar' must emit solar.earth/ files via prefix match"
    );
    assert!(
        !result.files.keys().any(|k| k.starts_with("echo/")),
        "filter 'solar' must not emit echo/ files"
    );
    assert!(
        !result.files.keys().any(|k| k.starts_with("health/")),
        "filter 'solar' must not emit health/ files"
    );
}

/// Multi-entry filter: each entry is independently matched.
#[test]
fn test_plugins_filter_multi_entry() {
    let ir = multi_ns_ir();
    let opts = GenerationOptions {
        generate: GenerateSelector::Plugins,
        plugins_filter: Some(vec!["echo".to_string(), "health".to_string()]),
        ..GenerationOptions::default()
    };
    let result = generate_typescript(&ir, &opts).unwrap();

    assert!(
        result.files.keys().any(|k| k.starts_with("echo/")),
        "filter ['echo','health'] must emit echo/ files"
    );
    assert!(
        result.files.keys().any(|k| k.starts_with("health/")),
        "filter ['echo','health'] must emit health/ files"
    );
    assert!(
        !result.files.keys().any(|k| k.starts_with("solar/")),
        "filter ['echo','health'] must not emit solar/ files"
    );
}

/// No filter (None): all namespaces are generated (regression guard).
#[test]
fn test_plugins_filter_none_generates_all() {
    let ir = multi_ns_ir();
    let opts = GenerationOptions {
        generate: GenerateSelector::Plugins,
        plugins_filter: None,
        ..GenerationOptions::default()
    };
    let result = generate_typescript(&ir, &opts).unwrap();

    assert!(result.files.keys().any(|k| k.starts_with("echo/")),   "no filter must emit echo/");
    assert!(result.files.keys().any(|k| k.starts_with("health/")), "no filter must emit health/");
    assert!(result.files.keys().any(|k| k.starts_with("solar/")),  "no filter must emit solar/");
}

/// Prefix must be dot-segment-aware: "sol" must NOT match "solar.earth".
#[test]
fn test_plugins_filter_no_substring_match() {
    let ir = multi_ns_ir();
    let opts = GenerationOptions {
        generate: GenerateSelector::Plugins,
        plugins_filter: Some(vec!["sol".to_string()]),
        ..GenerationOptions::default()
    };
    let result = generate_typescript(&ir, &opts).unwrap();

    assert!(
        result.files.is_empty(),
        "'sol' must not match 'solar.earth' (substring, not segment prefix); got: {:?}",
        result.files.keys().collect::<Vec<_>>()
    );
}

/// `--generate smoke`: single smoke.ts file using _info schema walk, no bun:test framework.
#[test]
fn test_selector_smoke_schema_walk() {
    let ir = minimal_ir();
    let opts = GenerationOptions { generate: GenerateSelector::Smoke, ..GenerationOptions::default() };
    let result = generate_typescript(&ir, &opts).unwrap();

    assert_eq!(result.files.len(), 1, "GenSmoke must emit exactly one file");
    let smoke = result.files.get("smoke.ts").expect("GenSmoke must emit smoke.ts");

    assert!(smoke.contains("_info"), "Schema walk smoke must call _info");
    assert!(smoke.contains(".schema"), "Schema walk smoke must call {{backend}}.schema");
    assert!(smoke.contains("activation_schema"), "Schema walk smoke must call activation_schema");
    assert!(!smoke.contains("bun:test"), "Schema walk smoke must not use bun:test framework");
    assert!(!smoke.contains("echo.ping"), "Schema walk smoke must not use domain-specific methods");
}

/// `--generate package`: only package.json.
#[test]
fn test_selector_package_only() {
    let ir = minimal_ir();
    let opts = GenerationOptions { generate: GenerateSelector::Package, ..GenerationOptions::default() };
    let result = generate_typescript(&ir, &opts).unwrap();

    assert_eq!(result.files.len(), 1, "GenPackage must emit exactly one file");
    assert!(result.files.contains_key("package.json"), "GenPackage must emit package.json");
}

/// `--generate all` (default): includes metadata file, all core artifacts.
#[test]
fn test_selector_all_includes_metadata() {
    let ir = minimal_ir();
    let result = generate_typescript(&ir, &GenerationOptions::default()).unwrap();

    assert!(result.files.contains_key(".codegen-metadata.json"), "GenAll must emit .codegen-metadata.json");
    assert!(result.files.contains_key("transport.ts"));
    assert!(result.files.contains_key("package.json"));
    assert!(result.files.contains_key("test/smoke.test.ts"));
}

// ─────────────────────────────────────────────────────────────
// Three-way merge
// ─────────────────────────────────────────────────────────────

/// Core safety guarantee: if a user edits a generated file, a subsequent
/// code generation run with the same IR must NOT overwrite their changes.
///
/// Failure here means users lose work on every rerun — the most important
/// invariant in the whole system.
#[test]
fn test_three_way_merge_preserves_user_edit() {
    use std::fs;

    let tmp = TempDir::new().unwrap();
    let out = tmp.path();
    let ir = minimal_ir();
    let opts = GenerationOptions::default();

    // Round 1: first generation — no cache yet, all files are new.
    let r1 = generate_typescript(&ir, &opts).unwrap();
    merge_generated_code(&r1.files, out, None, MergeStrategy::Skip).unwrap();

    // User edits rpc.ts.
    let rpc_path = out.join("rpc.ts");
    assert!(rpc_path.exists(), "rpc.ts must exist after first generation");
    let original = fs::read_to_string(&rpc_path).unwrap();
    let user_edit = format!("{}\n// User custom logic — must survive regeneration", original);
    fs::write(&rpc_path, &user_edit).unwrap();

    // Build a cache manifest that records the round-1 hashes.
    let manifest = cache_from_result(&r1.file_hashes);

    // Round 2: regenerate with the same IR (identical output).
    let r2 = generate_typescript(&ir, &opts).unwrap();
    let merge = merge_generated_code(&r2.files, out, Some(&manifest), MergeStrategy::Skip).unwrap();

    // The file must appear in `skipped`, not `updated`.
    let rpc_rel = std::path::PathBuf::from("rpc.ts");
    assert!(
        merge.skipped.contains(&rpc_rel),
        "rpc.ts should be skipped (user-modified), got skipped={:?} updated={:?}",
        merge.skipped,
        merge.updated
    );

    let after = fs::read_to_string(&rpc_path).unwrap();
    assert_eq!(after, user_edit, "User edit must be preserved by three-way merge");
}

/// `--force` semantics: user edits are overwritten when explicitly requested.
/// Without this, `--force` would silently fail to reset user changes.
#[test]
fn test_three_way_merge_force_overwrites_user_edit() {
    use std::fs;

    let tmp = TempDir::new().unwrap();
    let out = tmp.path();
    let ir = minimal_ir();
    let opts = GenerationOptions::default();

    let r1 = generate_typescript(&ir, &opts).unwrap();
    merge_generated_code(&r1.files, out, None, MergeStrategy::Skip).unwrap();

    let rpc_path = out.join("rpc.ts");
    let original = fs::read_to_string(&rpc_path).unwrap();
    let user_edit = format!("{}\n// This should be gone after --force", original);
    fs::write(&rpc_path, &user_edit).unwrap();

    let manifest = cache_from_result(&r1.file_hashes);

    let r2 = generate_typescript(&ir, &opts).unwrap();
    let merge = merge_generated_code(&r2.files, out, Some(&manifest), MergeStrategy::Force).unwrap();

    // File must appear in `updated`, not `skipped`.
    let rpc_rel = std::path::PathBuf::from("rpc.ts");
    assert!(
        merge.updated.contains(&rpc_rel),
        "rpc.ts should be updated (force), got updated={:?} skipped={:?}",
        merge.updated,
        merge.skipped
    );

    let after = fs::read_to_string(&rpc_path).unwrap();
    assert_ne!(after, user_edit, "Force must overwrite user edit");
    assert_eq!(
        compute_file_hash(&after),
        compute_file_hash(r2.files.get("rpc.ts").unwrap()),
        "Forced file must contain freshly generated content"
    );
}
