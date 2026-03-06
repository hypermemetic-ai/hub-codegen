//! TypeScript codegen invariant tests.
//!
//! Focused on invariants that matter for real-world correctness:
//!   1. Transport dispatch — none/browser/ws emit the right artifacts
//!   2. Three-way merge — user edits are preserved (Skip) or overwritten (Force)

use hub_codegen::cache::{CodeCacheManifest, CodePluginCache, ToolchainVersions};
use hub_codegen::generator::{GenerationOptions, TransportEnv};
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
        },
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
        ir_plugins,
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
            synapse_cc: "0.1.1".to_string(),
            synapse: "0.0.1".to_string(),
            hub_codegen: "0.1.1".to_string(),
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
    let result = generate_typescript(&ir, &GenerationOptions { transport: TransportEnv::None }).unwrap();

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
    let result = generate_typescript(&ir, &GenerationOptions { transport: TransportEnv::Browser }).unwrap();

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
    let result = generate_typescript(&ir, &GenerationOptions { transport: TransportEnv::Ws }).unwrap();

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
