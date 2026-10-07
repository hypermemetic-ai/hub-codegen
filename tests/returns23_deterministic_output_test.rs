//! RETURNS-23: the same IR and options give byte-identical output.
//!
//! The IR's `irMethods` / `irTypes` decode into std `HashMap`s, whose
//! iteration order is randomized per map (a fresh `RandomState` for every
//! map, a fresh process seed for every run). The Rust generator used to emit
//! each namespace's methods and types in that order, and both generators
//! listed their `warnings` in it.
//!
//! Every test here therefore decodes the fixture afresh for each generation
//! — reusing one `IR` would iterate the same maps the same way and hide the
//! bug — and the CLI test runs separate processes, as the ticket's
//! reproduction did. The fixture has several methods and types per
//! namespace (the reorder needs two or more), a nested namespace, unknown
//! types (warnings) and deprecations (deprecation warnings).

use hub_codegen::deprecation::DeprecationOptions;
use hub_codegen::generator::rust::generate_with_options as generate_rust_with_options;
use hub_codegen::{generate_typescript, GenerationOptions, GenerationResult, IR};
use std::path::PathBuf;
use std::process::Command;

/// How many fresh decodes each in-process test compares. With the old
/// behavior, sixteen decodes of this fixture agreeing by chance is
/// vanishingly unlikely (three namespaces of 3–7 methods and 2–4 types).
const RUNS: usize = 16;

const FIXTURE: &str = r#"{
  "irVersion": "2.0",
  "irBackend": "fixture",
  "irHash": "returns23-fixture",
  "irPlugins": {
    "": ["schema"],
    "arbor": ["create", "delete", "get", "list", "rename", "watch", "zap"],
    "arbor.tree": ["graft", "prune", "walk"],
    "bash": ["execute", "history", "kill", "status"]
  },
  "irMethods": {
    ".schema": {
      "mdName": "schema", "mdFullPath": ".schema", "mdNamespace": "",
      "mdParams": [], "mdReturns": {"tag": "RefUnknown"}
    },
    "arbor.create": {
      "mdName": "create", "mdFullPath": "arbor.create", "mdNamespace": "arbor",
      "mdDescription": "Create a node",
      "mdParams": [
        {"pdName": "label", "pdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "pdRequired": true},
        {"pdName": "kind", "pdType": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor", "qnLocalName": "NodeKind"}}, "pdRequired": true}
      ],
      "mdReturns": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor", "qnLocalName": "Node"}}
    },
    "arbor.delete": {
      "mdName": "delete", "mdFullPath": "arbor.delete", "mdNamespace": "arbor",
      "mdParams": [{"pdName": "id", "pdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "pdRequired": true}],
      "mdReturns": {"tag": "RefPrimitive", "contents": ["boolean", null]}
    },
    "arbor.get": {
      "mdName": "get", "mdFullPath": "arbor.get", "mdNamespace": "arbor",
      "mdParams": [
        {"pdName": "id", "pdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "pdRequired": true},
        {"pdName": "opts", "pdType": {"tag": "RefUnknown"}, "pdRequired": false}
      ],
      "mdReturns": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor", "qnLocalName": "Node"}}
    },
    "arbor.list": {
      "mdName": "list", "mdFullPath": "arbor.list", "mdNamespace": "arbor",
      "mdParams": [],
      "mdReturns": {"tag": "RefArray", "contents": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor", "qnLocalName": "Node"}}}
    },
    "arbor.rename": {
      "mdName": "rename", "mdFullPath": "arbor.rename", "mdNamespace": "arbor",
      "mdParams": [
        {"pdName": "id", "pdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "pdRequired": true},
        {"pdName": "label", "pdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "pdRequired": true},
        {"pdName": "force", "pdType": {"tag": "RefPrimitive", "contents": ["boolean", null]}, "pdRequired": false,
         "pdDeprecation": {"since": "0.5", "removedIn": "0.8", "message": "renames always apply"}}
      ],
      "mdReturns": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor", "qnLocalName": "Node"}}
    },
    "arbor.watch": {
      "mdName": "watch", "mdFullPath": "arbor.watch", "mdNamespace": "arbor",
      "mdStreaming": true, "mdParams": [],
      "mdReturns": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor", "qnLocalName": "TreeEvent"}}
    },
    "arbor.zap": {
      "mdName": "zap", "mdFullPath": "arbor.zap", "mdNamespace": "arbor",
      "mdParams": [{"pdName": "id", "pdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "pdRequired": true}],
      "mdReturns": {"tag": "RefUnknown"},
      "mdDeprecation": {"since": "0.5", "removedIn": "0.8", "message": "use delete"}
    },
    "arbor.tree.graft": {
      "mdName": "graft", "mdFullPath": "arbor.tree.graft", "mdNamespace": "arbor.tree",
      "mdParams": [{"pdName": "branch", "pdType": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor.tree", "qnLocalName": "Branch"}}, "pdRequired": true}],
      "mdReturns": {"tag": "RefUnknown"}
    },
    "arbor.tree.prune": {
      "mdName": "prune", "mdFullPath": "arbor.tree.prune", "mdNamespace": "arbor.tree",
      "mdParams": [{"pdName": "depth", "pdType": {"tag": "RefPrimitive", "contents": ["integer", null]}, "pdRequired": true}],
      "mdReturns": {"tag": "RefPrimitive", "contents": ["integer", null]}
    },
    "arbor.tree.walk": {
      "mdName": "walk", "mdFullPath": "arbor.tree.walk", "mdNamespace": "arbor.tree",
      "mdStreaming": true,
      "mdParams": [{"pdName": "root", "pdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "pdRequired": true}],
      "mdReturns": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor.tree", "qnLocalName": "WalkStep"}}
    },
    "bash.execute": {
      "mdName": "execute", "mdFullPath": "bash.execute", "mdNamespace": "bash",
      "mdStreaming": true,
      "mdParams": [{"pdName": "request", "pdType": {"tag": "RefNamed", "contents": {"qnNamespace": "bash", "qnLocalName": "ExecRequest"}}, "pdRequired": true}],
      "mdReturns": {"tag": "RefNamed", "contents": {"qnNamespace": "bash", "qnLocalName": "Output"}}
    },
    "bash.history": {
      "mdName": "history", "mdFullPath": "bash.history", "mdNamespace": "bash",
      "mdParams": [{"pdName": "limit", "pdType": {"tag": "RefOptional", "contents": {"tag": "RefPrimitive", "contents": ["integer", null]}}, "pdRequired": false}],
      "mdReturns": {"tag": "RefArray", "contents": {"tag": "RefPrimitive", "contents": ["string", null]}}
    },
    "bash.kill": {
      "mdName": "kill", "mdFullPath": "bash.kill", "mdNamespace": "bash",
      "mdParams": [
        {"pdName": "pid", "pdType": {"tag": "RefPrimitive", "contents": ["integer", null]}, "pdRequired": true},
        {"pdName": "signal", "pdType": {"tag": "RefNamed", "contents": {"qnNamespace": "bash", "qnLocalName": "Signal"}}, "pdRequired": true}
      ],
      "mdReturns": {"tag": "RefPrimitive", "contents": ["boolean", null]}
    },
    "bash.status": {
      "mdName": "status", "mdFullPath": "bash.status", "mdNamespace": "bash",
      "mdParams": [], "mdReturns": {"tag": "RefUnknown"}
    }
  },
  "irTypes": {
    "arbor.Node": {
      "tdName": "Node", "tdNamespace": "arbor", "tdDescription": "A node in the tree",
      "tdKind": {"tag": "KindStruct", "ksFields": [
        {"fdName": "id", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true},
        {"fdName": "label", "fdType": {"tag": "RefOptional", "contents": {"tag": "RefPrimitive", "contents": ["string", null]}}},
        {"fdName": "payload", "fdType": {"tag": "RefUnknown"}},
        {"fdName": "oldLabel", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]},
         "fdDeprecation": {"since": "0.5", "removedIn": "0.8", "message": "use label"}}
      ]}
    },
    "arbor.NodeKind": {
      "tdName": "NodeKind", "tdNamespace": "arbor",
      "tdKind": {"tag": "KindStringEnum", "kseValues": ["root", "branch", "leaf"]}
    },
    "arbor.TreeEvent": {
      "tdName": "TreeEvent", "tdNamespace": "arbor",
      "tdKind": {"tag": "KindEnum", "keDiscriminator": "type", "keVariants": [
        {"vdName": "created", "vdFields": [{"fdName": "node", "fdType": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor", "qnLocalName": "Node"}}, "fdRequired": true}]},
        {"vdName": "removed", "vdFields": [{"fdName": "id", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true}]},
        {"vdName": "annotated", "vdFields": [{"fdName": "note", "fdType": {"tag": "RefUnknown"}}]}
      ]}
    },
    "arbor.LegacyNode": {
      "tdName": "LegacyNode", "tdNamespace": "arbor",
      "tdDeprecation": {"since": "0.5", "removedIn": "0.8", "message": "use Node"},
      "tdKind": {"tag": "KindStruct", "ksFields": [
        {"fdName": "id", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true}
      ]}
    },
    "arbor.tree.Branch": {
      "tdName": "Branch", "tdNamespace": "arbor.tree",
      "tdKind": {"tag": "KindStruct", "ksFields": [
        {"fdName": "name", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true},
        {"fdName": "nodes", "fdType": {"tag": "RefArray", "contents": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor", "qnLocalName": "Node"}}}, "fdRequired": true},
        {"fdName": "weight", "fdType": {"tag": "RefPrimitive", "contents": ["number", null]}}
      ]}
    },
    "arbor.tree.WalkStep": {
      "tdName": "WalkStep", "tdNamespace": "arbor.tree",
      "tdKind": {"tag": "KindStruct", "ksFields": [
        {"fdName": "depth", "fdType": {"tag": "RefPrimitive", "contents": ["integer", null]}, "fdRequired": true},
        {"fdName": "node", "fdType": {"tag": "RefNamed", "contents": {"qnNamespace": "arbor", "qnLocalName": "Node"}}, "fdRequired": true},
        {"fdName": "hint", "fdType": {"tag": "RefArray", "contents": {"tag": "RefUnknown"}}}
      ]}
    },
    "bash.ExecRequest": {
      "tdName": "ExecRequest", "tdNamespace": "bash",
      "tdKind": {"tag": "KindStruct", "ksFields": [
        {"fdName": "command", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true},
        {"fdName": "env", "fdType": {"tag": "RefUnknown"}}
      ]}
    },
    "bash.Output": {
      "tdName": "Output", "tdNamespace": "bash",
      "tdKind": {"tag": "KindEnum", "keDiscriminator": "type", "keVariants": [
        {"vdName": "stdout", "vdFields": [{"fdName": "line", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true}]},
        {"vdName": "stderr", "vdFields": [{"fdName": "line", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true}]},
        {"vdName": "exit", "vdFields": [{"fdName": "code", "fdType": {"tag": "RefPrimitive", "contents": ["integer", null]}, "fdRequired": true}]}
      ]}
    },
    "bash.Signal": {
      "tdName": "Signal", "tdNamespace": "bash",
      "tdKind": {"tag": "KindStringEnum", "kseValues": ["term", "kill", "int"]}
    }
  }
}"#;

/// Decode the fixture afresh: new `HashMap`s, new iteration order.
fn fresh_ir() -> IR {
    serde_json::from_str(FIXTURE).expect("fixture IR decodes")
}

/// The CLI's default: deprecation annotations on.
fn generate_rust(ir: &IR) -> GenerationResult {
    generate_rust_with_options(ir, DeprecationOptions { enabled: true }).expect("rust generation")
}

fn generate_ts(ir: &IR) -> GenerationResult {
    generate_typescript(ir, &GenerationOptions::default()).expect("typescript generation")
}

/// Every field of a result that a consumer sees, with the warning lists
/// flattened to their printed form so order differences show up.
fn assert_same(first: &GenerationResult, other: &GenerationResult, target: &str, run: usize) {
    assert_eq!(first.files, other.files, "{target}: files differ on run {run}");
    assert_eq!(first.file_hashes, other.file_hashes, "{target}: fileHashes differ on run {run}");
    let warnings = |r: &GenerationResult| r.warnings.iter().map(|w| w.to_string()).collect::<Vec<_>>();
    assert_eq!(warnings(first), warnings(other), "{target}: warnings order differs on run {run}");
    assert_eq!(
        first.deprecation_warnings, other.deprecation_warnings,
        "{target}: deprecation warnings order differs on run {run}"
    );
}

// ─────────────────────────────────────────────────────────────
// Acceptance 1 — Rust: byte-identical files and fileHashes
// ─────────────────────────────────────────────────────────────

#[test]
fn rust_output_is_identical_across_fresh_decodes() {
    let first = generate_rust(&fresh_ir());
    // The fixture must exercise what used to vary.
    assert!(first.files.contains_key("src/arbor/mod.rs"));
    assert!(first.files.contains_key("src/arbor/tree/mod.rs"));
    assert!(first.files.contains_key("src/bash/mod.rs"));
    assert!(!first.warnings.is_empty());
    assert!(first.deprecation_warnings.len() >= 4);

    for run in 1..RUNS {
        assert_same(&first, &generate_rust(&fresh_ir()), "rust", run);
    }
}

/// Methods and types within a namespace come out sorted by name.
#[test]
fn rust_namespace_members_are_sorted_by_name() {
    let result = generate_rust(&fresh_ir());
    let module = &result.files["src/arbor/mod.rs"];

    let position = |needle: &str| {
        module
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` missing from src/arbor/mod.rs:\n{module}"))
    };
    let types: Vec<usize> = ["pub struct LegacyNode", "pub struct Node", "pub enum NodeKind", "pub enum TreeEvent"]
        .iter()
        .map(|t| position(t))
        .collect();
    assert!(types.windows(2).all(|w| w[0] < w[1]), "types not in name order:\n{module}");

    let methods: Vec<usize> = ["create", "delete", "get", "list", "rename", "watch", "zap"]
        .iter()
        .map(|m| position(&format!("pub async fn {m}(")))
        .collect();
    assert!(methods.windows(2).all(|w| w[0] < w[1]), "methods not in name order:\n{module}");
}

// ─────────────────────────────────────────────────────────────
// Acceptance 2 — TypeScript: identical warnings (and files)
// ─────────────────────────────────────────────────────────────

#[test]
fn typescript_output_is_identical_across_fresh_decodes() {
    let first = generate_ts(&fresh_ir());
    assert!(first.warnings.len() >= 8, "fixture should raise several warnings");

    for run in 1..RUNS {
        assert_same(&first, &generate_ts(&fresh_ir()), "typescript", run);
    }
}

/// Both generators return their warnings sorted (location, then message).
#[test]
fn warnings_are_sorted() {
    for (target, result) in [("rust", generate_rust(&fresh_ir())), ("typescript", generate_ts(&fresh_ir()))] {
        let mut sorted = result.warnings.clone();
        sorted.sort();
        assert_eq!(result.warnings, sorted, "{target} warnings not sorted");
        let locations: Vec<&str> = result.warnings.iter().map(|w| w.location.as_str()).collect();
        assert_eq!(
            locations,
            vec![
                ".schema",
                "arbor.Node.payload",
                "arbor.TreeEvent.annotated.note",
                "arbor.get(opts)",
                "arbor.tree.WalkStep.hint",
                "arbor.tree.graft",
                "arbor.zap",
                "bash.ExecRequest.env",
                "bash.status",
            ],
            "{target} warning locations"
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Golden — the multi-method, multi-type namespace modules
// ─────────────────────────────────────────────────────────────

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("returns23")
}

/// Compare against the committed golden `name`; `UPDATE_GOLDEN=1` rewrites it.
fn assert_golden(name: &str, actual: &str) {
    let path = golden_dir().join(name);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(golden_dir()).expect("create golden dir");
        std::fs::write(&path, actual).expect("write golden file");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("missing golden file {} ({e}); regenerate with UPDATE_GOLDEN=1 cargo test", path.display())
    });
    assert_eq!(
        actual,
        expected,
        "generated output diverged from golden {} — if intentional, regenerate with UPDATE_GOLDEN=1 cargo test",
        path.display()
    );
}

#[test]
fn rust_namespace_modules_match_golden() {
    let result = generate_rust(&fresh_ir());
    assert_golden("rust_arbor_mod.rs", &result.files["src/arbor/mod.rs"]);
    assert_golden("rust_arbor_tree_mod.rs", &result.files["src/arbor/tree/mod.rs"]);
    assert_golden("rust_bash_mod.rs", &result.files["src/bash/mod.rs"]);
}

// ─────────────────────────────────────────────────────────────
// The CLI across processes — the ticket's reproduction
// ─────────────────────────────────────────────────────────────

/// `hub-codegen --output-format json -t <target> ir.json`, run as separate
/// processes (each with its own hash seed), prints byte-identical stdout —
/// maps in key order, warnings sorted — and byte-identical stderr.
#[test]
fn cli_json_output_is_byte_identical_across_processes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ir_path = dir.path().join("ir.json");
    std::fs::write(&ir_path, FIXTURE).expect("write fixture");

    for target in ["rust", "typescript"] {
        let run = || {
            let out = Command::new(env!("CARGO_BIN_EXE_hub-codegen"))
                .args(["--output-format", "json", "-t", target])
                .arg(&ir_path)
                .output()
                .expect("run hub-codegen");
            assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
            (out.stdout, out.stderr)
        };
        let (stdout, stderr) = run();

        // Maps serialize in key order.
        let parsed: serde_json::Value = serde_json::from_slice(&stdout).expect("json stdout");
        let printed = String::from_utf8(stdout.clone()).unwrap();
        let keys: Vec<&String> = parsed["files"].as_object().unwrap().keys().collect();
        let mut offsets = keys.iter().map(|k| printed.find(&format!("{:?}:", k)).expect("key printed"));
        let mut last = offsets.next().unwrap();
        for at in offsets {
            assert!(at > last, "{target}: files not printed in key order");
            last = at;
        }

        for i in 1..5 {
            let (again_out, again_err) = run();
            assert!(again_out == stdout, "{target}: stdout differs on process {i}");
            assert!(again_err == stderr, "{target}: stderr differs on process {i}");
        }
    }
}
