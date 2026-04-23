//! IR-7 integration tests: deprecation annotations in generated code.
//!
//! Covers the 7 acceptance criteria from the ticket:
//!
//! 1. TS annotation — `@deprecated` JSDoc and `// DEPRECATED` comment
//!    with the pinned body format.
//! 2. Rust annotation — `#[deprecated(since = ..., note = ...)]` attribute
//!    with identical body content.
//! 3. Stderr warning contains `WARNING`, method name, `0.5`, `0.7`, and
//!    the message substring.
//! 4. `--fail-on-deprecated` exits non-zero; generated files still on disk.
//! 5. `--no-deprecation-annotations` produces zero annotations and zero
//!    stderr warnings.
//! 6. Pre-IR regression — byte-identical output to pre-ticket for a
//!    fixture with no deprecation fields.
//! 7. Determinism — two consecutive regens produce byte-identical files.

use hub_codegen::deprecation::DeprecationOptions;
use hub_codegen::generator::{GenerationOptions, TransportEnv};
use hub_codegen::ir::*;
use hub_codegen::{generate_typescript, IR};
use std::collections::HashMap;

/// Build an IR fixture with one post-IR deprecated method:
/// - `echo.old_ping`: deprecated, since "0.5", removed_in "0.7",
///   message "use foo2"
/// - `echo.new_ping`: not deprecated
fn post_ir_fixture() -> IR {
    let mut ir_methods = HashMap::new();

    ir_methods.insert(
        "echo.old_ping".to_string(),
        MethodDef {
            md_name: "old_ping".to_string(),
            md_full_path: "echo.old_ping".to_string(),
            md_namespace: "echo".to_string(),
            md_description: Some("Legacy ping".to_string()),
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: Default::default(),
            md_deprecation: Some(DeprecationInfo {
                since: "0.5".to_string(),
                removed_in: "0.7".to_string(),
                message: "use foo2".to_string(),
            }),
        },
    );

    ir_methods.insert(
        "echo.new_ping".to_string(),
        MethodDef {
            md_name: "new_ping".to_string(),
            md_full_path: "echo.new_ping".to_string(),
            md_namespace: "echo".to_string(),
            md_description: Some("Current ping".to_string()),
            md_streaming: false,
            md_params: vec![],
            md_returns: TypeRef::RefPrimitive("string".to_string(), None),
            md_bidir_type: None,
            md_role: Default::default(),
            md_deprecation: None,
        },
    );

    let mut ir_plugins = HashMap::new();
    ir_plugins.insert(
        "echo".to_string(),
        vec!["old_ping".to_string(), "new_ping".to_string()],
    );

    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("ir7-fixture-abc".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods,
        ir_plugins,
        ir_plugin_deprecations: HashMap::new(),
        ir_plugin_requests: HashMap::new(),
    }
}

/// Pre-IR fixture: same shape, but every deprecation field is `None`.
fn pre_ir_fixture() -> IR {
    let mut ir = post_ir_fixture();
    for m in ir.ir_methods.values_mut() {
        m.md_deprecation = None;
    }
    ir.ir_plugin_deprecations.clear();
    ir
}

fn default_options() -> GenerationOptions {
    GenerationOptions {
        transport: TransportEnv::Ws,
        ..GenerationOptions::default()
    }
}

// ─────────────────────────────────────────────────────────────
// Test 1 — TypeScript annotation
// ─────────────────────────────────────────────────────────────

#[test]
fn test_ts_emits_deprecated_jsdoc_and_comment() {
    let ir = post_ir_fixture();
    let result = generate_typescript(&ir, &default_options()).unwrap();

    // The echo namespace client should contain both annotations above
    // the deprecated method.
    let echo_client = result
        .files
        .get("echo/client.ts")
        .expect("echo/client.ts should be generated");

    assert!(
        echo_client.contains("@deprecated since 0.5, removed in 0.7: use foo2"),
        "Expected @deprecated JSDoc. Got:\n{}",
        echo_client
    );
    assert!(
        echo_client.contains("// DEPRECATED since 0.5, removed in 0.7: use foo2"),
        "Expected // DEPRECATED comment. Got:\n{}",
        echo_client
    );
}

// ─────────────────────────────────────────────────────────────
// Test 2 — Rust annotation
// ─────────────────────────────────────────────────────────────

#[cfg(feature = "rust")]
#[test]
fn test_rust_emits_deprecated_attribute() {
    use hub_codegen::generator::rust::generate_with_options;

    let ir = post_ir_fixture();
    let result = generate_with_options(&ir, DeprecationOptions { enabled: true }).unwrap();

    // Find the echo module file and check for the attribute.
    let (_, echo_mod) = result
        .files
        .iter()
        .find(|(k, _)| k.contains("echo"))
        .expect("echo module should be generated");

    assert!(
        echo_mod.contains("#[deprecated(since = \"0.5\", note = \"use foo2 (removed in 0.7)\")]"),
        "Expected Rust #[deprecated] attribute. Got:\n{}",
        echo_mod
    );
}

// ─────────────────────────────────────────────────────────────
// Test 3 — stderr warning content
// ─────────────────────────────────────────────────────────────

#[test]
fn test_stderr_warning_contains_expected_substrings() {
    let ir = post_ir_fixture();
    let result = generate_typescript(&ir, &default_options()).unwrap();

    // At least one deprecation warning should be present.
    assert!(
        !result.deprecation_warnings.is_empty(),
        "Expected at least one deprecation warning, got none"
    );

    // Each warning's stderr format must include the required substrings.
    let any_match = result.deprecation_warnings.iter().any(|w| {
        let line = w.format_stderr();
        line.contains("WARNING")
            && line.contains("echo.old_ping")
            && line.contains("0.5")
            && line.contains("0.7")
            && line.contains("use foo2")
    });
    assert!(
        any_match,
        "No warning line contained all expected substrings. Got:\n{:#?}",
        result
            .deprecation_warnings
            .iter()
            .map(|w| w.format_stderr())
            .collect::<Vec<_>>()
    );
}

// ─────────────────────────────────────────────────────────────
// Test 4 — --fail-on-deprecated semantics (verified at library layer)
// ─────────────────────────────────────────────────────────────

#[test]
fn test_fail_on_deprecated_records_warnings_and_writes_files() {
    // --fail-on-deprecated is a CLI flag that escalates the process
    // exit code *after* files are written. At the library boundary,
    // the invariant we can verify mechanically is that when the IR
    // is post-IR and annotations are enabled:
    //   (a) generated files still contain their post-IR content
    //   (b) the deprecation_warnings vector is non-empty
    // The CLI main() then checks (b) to decide whether to exit(2).
    let ir = post_ir_fixture();
    let result = generate_typescript(&ir, &default_options()).unwrap();

    // Files are still written.
    assert!(!result.files.is_empty(), "Expected generated files");
    let echo_client = result
        .files
        .get("echo/client.ts")
        .expect("echo/client.ts present");
    assert!(echo_client.contains("oldPing"));

    // deprecation_warnings captured so main() can escalate.
    assert!(
        !result.deprecation_warnings.is_empty(),
        "Expected deprecation_warnings for --fail-on-deprecated escalation"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 5 — --no-deprecation-annotations suppresses everything
// ─────────────────────────────────────────────────────────────

#[test]
fn test_no_deprecation_annotations_suppresses_output() {
    let ir = post_ir_fixture();
    let opts = GenerationOptions {
        deprecation: DeprecationOptions { enabled: false },
        ..default_options()
    };
    let result = generate_typescript(&ir, &opts).unwrap();

    // Zero deprecation warnings recorded.
    assert!(
        result.deprecation_warnings.is_empty(),
        "Expected zero deprecation warnings when suppressed, got: {:?}",
        result
            .deprecation_warnings
            .iter()
            .map(|w| w.format_stderr())
            .collect::<Vec<_>>()
    );

    // Zero annotations in generated files.
    for (path, content) in &result.files {
        assert!(
            !content.contains("@deprecated"),
            "File {} contained @deprecated when suppressed. Content:\n{}",
            path,
            content
        );
        assert!(
            !content.contains("// DEPRECATED "),
            "File {} contained // DEPRECATED when suppressed",
            path
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Test 6 — Pre-IR regression: output byte-identical to pre-ticket
// ─────────────────────────────────────────────────────────────

#[test]
fn test_pre_ir_byte_identical_output() {
    // Compare a pre-IR fixture run WITH annotations enabled (the default)
    // against the same fixture run with annotations disabled. Because the
    // fixture carries no deprecation fields and ir_version < 0.5, the
    // post-IR detector should classify it as pre-IR and both runs must
    // produce identical output.
    let ir = pre_ir_fixture();

    let mut opts_default = default_options();
    opts_default.deprecation = DeprecationOptions { enabled: true };
    let result_default = generate_typescript(&ir, &opts_default).unwrap();

    let mut opts_suppressed = default_options();
    opts_suppressed.deprecation = DeprecationOptions { enabled: false };
    let result_suppressed = generate_typescript(&ir, &opts_suppressed).unwrap();

    assert!(
        result_default.deprecation_warnings.is_empty(),
        "Pre-IR fixture must produce zero deprecation warnings"
    );

    // Byte-identical file contents.
    let mut lhs: Vec<_> = result_default.files.iter().collect();
    lhs.sort_by_key(|(k, _)| k.to_string());
    let mut rhs: Vec<_> = result_suppressed.files.iter().collect();
    rhs.sort_by_key(|(k, _)| k.to_string());
    assert_eq!(
        lhs.len(),
        rhs.len(),
        "Pre-IR fixture should produce same file count with/without annotations"
    );
    for ((lp, lc), (rp, rc)) in lhs.iter().zip(rhs.iter()) {
        assert_eq!(lp, rp, "file key differed");
        assert_eq!(lc, rc, "file {} differed between annotation modes", lp);
    }

    // Additionally, pre-IR output must contain NO deprecation annotations.
    for (path, content) in &result_default.files {
        assert!(
            !content.contains("@deprecated"),
            "Pre-IR file {} unexpectedly contains @deprecated",
            path
        );
        assert!(
            !content.contains("// DEPRECATED "),
            "Pre-IR file {} unexpectedly contains // DEPRECATED",
            path
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Test 7 — Determinism
// ─────────────────────────────────────────────────────────────

#[test]
fn test_determinism_two_regens_identical() {
    let ir = post_ir_fixture();
    let result1 = generate_typescript(&ir, &default_options()).unwrap();
    let result2 = generate_typescript(&ir, &default_options()).unwrap();

    assert_eq!(
        result1.files.len(),
        result2.files.len(),
        "Regen produced different file counts"
    );

    let mut keys1: Vec<_> = result1.files.keys().collect();
    let mut keys2: Vec<_> = result2.files.keys().collect();
    keys1.sort();
    keys2.sort();
    assert_eq!(keys1, keys2, "Regen produced different file sets");

    for key in keys1 {
        let c1 = &result1.files[key];
        let c2 = &result2.files[key];
        assert_eq!(c1, c2, "Regen produced different content for {}", key);
    }

    // Deprecation warnings are also deterministic.
    let mut w1: Vec<String> = result1
        .deprecation_warnings
        .iter()
        .map(|w| w.format_stderr())
        .collect();
    let mut w2: Vec<String> = result2
        .deprecation_warnings
        .iter()
        .map(|w| w.format_stderr())
        .collect();
    w1.sort();
    w2.sort();
    assert_eq!(w1, w2, "Regen produced different deprecation warnings");
}

// ─────────────────────────────────────────────────────────────
// Bonus: plugin-level + param-level deprecation coverage
// ─────────────────────────────────────────────────────────────

#[test]
fn test_plugin_level_deprecation_annotates_client_interface() {
    let mut ir = pre_ir_fixture();
    ir.ir_plugin_deprecations.insert(
        "echo".to_string(),
        DeprecationInfo {
            since: "0.5".to_string(),
            removed_in: "0.7".to_string(),
            message: "switch to echo2".to_string(),
        },
    );

    let result = generate_typescript(&ir, &default_options()).unwrap();
    let echo_client = result.files.get("echo/client.ts").unwrap();
    assert!(
        echo_client.contains("@deprecated since 0.5, removed in 0.7: switch to echo2"),
        "Expected plugin-level @deprecated. Got:\n{}",
        echo_client
    );
    assert!(
        result
            .deprecation_warnings
            .iter()
            .any(|w| w.kind == "plugin" && w.name == "echo"),
        "Expected a plugin-kind deprecation warning"
    );
}

// ─────────────────────────────────────────────────────────────
// CLI end-to-end: --fail-on-deprecated exits non-zero
// ─────────────────────────────────────────────────────────────

/// Acceptance 5: running the regen with `--fail-on-deprecated` against a
/// post-IR schema must exit non-zero, and the generated files must still
/// be on disk. We drive the real CLI binary to verify.
#[test]
fn test_cli_fail_on_deprecated_exits_nonzero_and_writes_files() {
    use std::process::Command;
    use tempfile::TempDir;

    let out_dir = TempDir::new().expect("tempdir");
    let ir_path = out_dir.path().join("fixture.json");
    let out_path = out_dir.path().join("out");

    // Build the IR JSON directly in the Haskell-tag format that hub-codegen's
    // custom TypeRef deserializer expects. Serializing the Rust struct with
    // serde's default encoding would emit the wrong shape.
    let ir_json = serde_json::json!({
        "irVersion": "2.0",
        "irBackend": "test",
        "irHash": "ir7-fixture-abc",
        "irTypes": {},
        "irMethods": {
            "echo.old_ping": {
                "mdName": "old_ping",
                "mdFullPath": "echo.old_ping",
                "mdNamespace": "echo",
                "mdDescription": "Legacy ping",
                "mdStreaming": false,
                "mdParams": [],
                "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]},
                "mdDeprecation": {
                    "since": "0.5",
                    "removedIn": "0.7",
                    "message": "use foo2"
                }
            },
            "echo.new_ping": {
                "mdName": "new_ping",
                "mdFullPath": "echo.new_ping",
                "mdNamespace": "echo",
                "mdDescription": "Current ping",
                "mdStreaming": false,
                "mdParams": [],
                "mdReturns": {"tag": "RefPrimitive", "contents": ["string", null]}
            }
        },
        "irPlugins": {
            "echo": ["old_ping", "new_ping"]
        }
    });
    std::fs::write(&ir_path, serde_json::to_string(&ir_json).unwrap()).expect("write ir");

    // Build the CLI binary first, then invoke it. Using cargo run ensures
    // the same workspace is used as the library tests.
    let status = Command::new(env!("CARGO_BIN_EXE_hub-codegen"))
        .arg(&ir_path)
        .arg("--output")
        .arg(&out_path)
        .arg("--target")
        .arg("typescript")
        .arg("--fail-on-deprecated")
        .output()
        .expect("run hub-codegen");

    assert!(
        !status.status.success(),
        "Expected non-zero exit, got {:?}. stdout:\n{}\nstderr:\n{}",
        status.status,
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr),
    );

    // stderr must contain the WARNING line.
    let stderr = String::from_utf8_lossy(&status.stderr);
    assert!(
        stderr.contains("WARNING"),
        "Expected WARNING on stderr, got:\n{}",
        stderr
    );
    assert!(
        stderr.contains("echo.old_ping"),
        "Expected method name on stderr"
    );
    assert!(stderr.contains("0.5"));
    assert!(stderr.contains("0.7"));
    assert!(stderr.contains("use foo2"));

    // Files still written to disk.
    let client_path = out_path.join("echo").join("client.ts");
    assert!(
        client_path.exists(),
        "Generated file was not written despite --fail-on-deprecated: {:?}",
        client_path
    );
}

#[test]
fn test_param_level_deprecation_annotates_parameter() {
    let mut ir = pre_ir_fixture();
    // Add a deprecated param to new_ping.
    let m = ir.ir_methods.get_mut("echo.new_ping").unwrap();
    m.md_params.push(ParamDef {
        pd_name: "legacy_flag".to_string(),
        pd_type: TypeRef::RefPrimitive("boolean".to_string(), None),
        pd_description: None,
        pd_required: false,
        pd_default: None,
        pd_deprecation: Some(DeprecationInfo {
            since: "0.5".to_string(),
            removed_in: "0.7".to_string(),
            message: "no longer honored".to_string(),
        }),
        pd_source: None,
    });

    let result = generate_typescript(&ir, &default_options()).unwrap();
    let echo_client = result.files.get("echo/client.ts").unwrap();
    assert!(
        echo_client.contains("/* @deprecated since 0.5, removed in 0.7: no longer honored */"),
        "Expected parameter-level /* @deprecated */ inline. Got:\n{}",
        echo_client
    );
    assert!(
        result
            .deprecation_warnings
            .iter()
            .any(|w| w.kind == "param" && w.name.contains("legacy_flag")),
        "Expected a param-kind deprecation warning"
    );
}
