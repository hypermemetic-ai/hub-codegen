//! RETURNS-38 · `watch(a, b, c)` in the generated TypeScript client.
//!
//! 1. `watch.ts` is a static runtime module, emitted beside `rpc.ts` and
//!    re-exported from `index.ts`; its golden is `golden/returns38/watch.ts`.
//! 2. Everything else is what it was: every file hub-codegen@6876246 (main,
//!    before this) generated for the two fixtures — TypeScript under each
//!    transport, and Rust — hashes the same today
//!    (`golden/returns38/before.sha256`), but for `watch.ts` (new),
//!    `index.ts` (its export, and only that), `transport.ts` (the
//!    unsubscribe; its golden is `golden/ca2/ts_transport_spinner.ts`),
//!    `package.json` (its version is a hash of the content) and
//!    `.codegen-metadata.json` (the file hashes).
//! 3. The TypeScript tests in `tests/ts_watch/` — the type of the merge
//!    (`types.test.ts`, under `tsc`, `@ts-expect-error` as assertions) and
//!    its behaviour on a mock socket (`runtime.test.ts`, under
//!    `node --test`) — run against a package generated from
//!    `fixtures/returns38_watch_ir.json`. They need Node and the harness's
//!    `npm install` (done here when `node_modules` is missing); without
//!    Node they are skipped, unless `HUB_CODEGEN_REQUIRE_TS=1`.

use hub_codegen::generator::{GenerationOptions, TransportEnv};
use hub_codegen::{generate_rust, generate_typescript, IR};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> IR {
    let path = root().join("tests/fixtures").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("read fixture")).expect("the fixture decodes")
}

fn ts(ir: &IR, transport: TransportEnv) -> HashMap<String, String> {
    generate_typescript(ir, &GenerationOptions { transport, ..GenerationOptions::default() })
        .expect("generate typescript")
        .files
}

/// sha256 of a file's blocks (split on blank lines), sorted: main's Rust
/// generator orders its blocks by `HashMap`, so only the set is stable.
fn sha256(s: &str) -> String {
    let mut blocks: Vec<&str> = s.split("\n\n").collect();
    blocks.sort();
    Sha256::digest(blocks.join("\n\n").as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

const WATCH_EXPORT: &str = "// Many streams as one: watch(a, b, c)\nexport * from './watch';\n\n";

// ─────────────────────────────────────────────────────────────
// 1. watch.ts
// ─────────────────────────────────────────────────────────────

#[test]
fn watch_ts_golden() {
    let files = ts(&fixture("returns38_watch_ir"), TransportEnv::Browser);
    let actual = files.get("watch.ts").expect("watch.ts is emitted");
    let path = root().join("tests/golden/returns38/watch.ts");
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).expect("golden watch.ts");
    assert_eq!(actual, &expected, "watch.ts diverged from its golden (UPDATE_GOLDEN=1 to accept)");
}

#[test]
fn watch_ts_is_emitted_and_exported_under_every_transport() {
    let ir = fixture("returns38_watch_ir");
    for transport in [TransportEnv::Ws, TransportEnv::Browser, TransportEnv::None] {
        let files = ts(&ir, transport);
        assert!(files.contains_key("watch.ts"), "{transport:?}");
        assert!(files["index.ts"].contains(WATCH_EXPORT), "{transport:?}: {}", files["index.ts"]);
        // the same file whatever the IR
        assert_eq!(files["watch.ts"], ts(&fixture("synapse_cc_2_injected_ir"), transport)["watch.ts"]);
        if transport != TransportEnv::None {
            let t = &files["transport.ts"];
            assert!(t.contains("import { currentCallScope } from './watch';"));
            assert!(t.contains("const scope = currentCallScope();"));
            assert!(t.contains(".call_unsub`, params: [subscriptionId]"));
        }
    }
}

// ─────────────────────────────────────────────────────────────
// 2. Everything else unchanged
// ─────────────────────────────────────────────────────────────

/// `<fixture>-<target>/<path>` → sha256, as main generated them.
fn before() -> BTreeMap<String, String> {
    std::fs::read_to_string(root().join("tests/golden/returns38/before.sha256"))
        .expect("before.sha256")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (hash, path) = l.split_once("  ").expect("sha256sum line");
            (path.to_string(), hash.to_string())
        })
        .collect()
}

#[test]
fn existing_output_unchanged() {
    let before = before();
    let mut compared = 0;
    for fx in ["returns38_watch_ir", "synapse_cc_2_injected_ir"] {
        let ir = fixture(fx);
        let mut outputs: Vec<(String, HashMap<String, String>)> = [
            ("ts-ws", TransportEnv::Ws),
            ("ts-browser", TransportEnv::Browser),
            ("ts-none", TransportEnv::None),
        ]
        .into_iter()
        .map(|(name, t)| (format!("{fx}-{name}"), ts(&ir, t)))
        .collect();
        outputs.push((format!("{fx}-rust"), generate_rust(&ir).expect("rust").files));

        for (dir, files) in outputs {
            let had: BTreeMap<&str, &str> = before
                .iter()
                .filter_map(|(p, h)| p.strip_prefix(&format!("{dir}/")).map(|rest| (rest, h.as_str())))
                .collect();
            assert!(!had.is_empty(), "no before-hashes for {dir}");
            for (path, hash) in &had {
                if ["package.json", ".codegen-metadata.json", "transport.ts"].contains(path) {
                    continue;
                }
                if dir.ends_with("-rust") && !files.contains_key(*path) {
                    // written by the CLI, not the generator (Cargo.toml / package.json scaffolding)
                    continue;
                }
                let now = files.get(*path).unwrap_or_else(|| panic!("{dir}/{path} is no longer generated"));
                let now = if *path == "index.ts" {
                    assert!(now.contains(WATCH_EXPORT), "{dir}/index.ts lacks the watch export");
                    now.replacen(WATCH_EXPORT, "", 1)
                } else {
                    now.clone()
                };
                assert_eq!(&sha256(&now), hash, "{dir}/{path} changed — every existing method's output must stay byte for byte");
                compared += 1;
            }
            for path in files.keys() {
                assert!(
                    had.contains_key(path.as_str()) || path == "watch.ts",
                    "{dir}/{path} is new; only watch.ts may be"
                );
            }
            if dir.contains("-ts-") {
                assert!(files.contains_key("watch.ts"), "{dir}");
            } else {
                assert!(!files.keys().any(|p| p.contains("watch")), "the Rust target is untouched: {dir}");
            }
        }
    }
    assert!(compared > 60, "compared {compared} files");
}

// ─────────────────────────────────────────────────────────────
// 3. The TypeScript tests
// ─────────────────────────────────────────────────────────────

fn harness() -> PathBuf {
    root().join("tests/ts_watch")
}

fn run(cmd: &mut Command) -> (bool, String) {
    let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// A generated package with the harness's tests beside it, compiled to
/// CommonJS by `tsc` (which also type-checks `types.test.ts`). `None` when
/// there is no Node (and it is not required).
fn compiled() -> Option<(tempfile::TempDir, PathBuf)> {
    let node_ok = Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !node_ok {
        assert!(std::env::var("HUB_CODEGEN_REQUIRE_TS").is_err(), "HUB_CODEGEN_REQUIRE_TS=1 and there is no node");
        eprintln!("node not available; skipping the TypeScript watch tests");
        return None;
    }
    let h = harness();
    if !h.join("node_modules/.bin/tsc").exists() {
        let (ok, text) = run(Command::new("npm").args(["ci", "--no-audit", "--no-fund"]).current_dir(&h));
        assert!(ok, "npm ci in {}: {text}", h.display());
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = tmp.path().join("pkg");
    for (path, content) in ts(&fixture("returns38_watch_ir"), TransportEnv::Browser) {
        let p = pkg.join(&path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    let tests = pkg.join("watch-tests");
    std::fs::create_dir_all(&tests).unwrap();
    for f in ["types.test.ts", "runtime.test.ts", "perf.ts"] {
        std::fs::copy(h.join(f), tests.join(f)).unwrap();
    }
    std::os::unix::fs::symlink(h.join("node_modules"), pkg.join("node_modules")).unwrap();
    let tsconfig = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "commonjs",
    "moduleResolution": "node10",
    "lib": ["ES2022", "DOM"],
    "types": ["node"],
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "outDir": "out"
  },
  "include": ["*.ts", "alpha/**/*.ts", "beta/**/*.ts", "gamma/**/*.ts", "watch-tests/*.ts"]
}"#;
    std::fs::write(pkg.join("tsconfig.watch.json"), tsconfig).unwrap();
    let (ok, text) = run(Command::new(h.join("node_modules/.bin/tsc")).args(["-p", "tsconfig.watch.json"]).current_dir(&pkg));
    assert!(ok, "tsc rejected the generated package or the watch tests:\n{text}");
    // the generated package.json says "type": "module"; its compiled copy is CommonJS
    std::fs::write(pkg.join("out/package.json"), r#"{ "type": "commonjs" }"#).unwrap();
    Some((tmp, pkg))
}

fn in_pkg(pkg: &Path, rel: &str) -> PathBuf {
    pkg.join("out").join(rel)
}

#[test]
fn ts_watch_types_and_behaviour() {
    let Some((_tmp, pkg)) = compiled() else { return };
    // types.test.ts is checked by the compile above: its `@ts-expect-error`
    // lines fail it when they are accepted.
    let (ok, text) = run(Command::new("node").args(["--test", "--test-reporter=spec"]).arg(in_pkg(&pkg, "watch-tests/runtime.test.js")).current_dir(&pkg));
    println!("{text}");
    assert!(ok, "node --test runtime.test.js failed:\n{text}");
    assert!(text.contains("pass 12"), "12 runtime tests:\n{text}");
}

/// The widening assertions are real: with them unmarked, `tsc` fails on
/// exactly those lines.
#[test]
fn ts_widening_is_a_type_error() {
    let Some((_tmp, pkg)) = compiled() else { return };
    let types = pkg.join("watch-tests/types.test.ts");
    let src = std::fs::read_to_string(&types).unwrap();
    let marked = src.matches("// @ts-expect-error").count();
    assert!(marked >= 10, "{marked} expected errors");
    std::fs::write(&types, src.replace("// @ts-expect-error", "// (unmarked)")).unwrap();
    let (ok, text) = run(
        Command::new(harness().join("node_modules/.bin/tsc"))
            .args(["-p", "tsconfig.watch.json", "--noEmit"])
            .current_dir(&pkg),
    );
    assert!(!ok, "unmarked, the type tests must fail");
    let errors = text.lines().filter(|l| l.contains("watch-tests/types.test.ts") && l.contains("error TS")).count();
    assert_eq!(errors, marked, "one error per marked line:\n{text}");
}

/// Perf: `cargo test --release --test returns38_watch_test -- --ignored --nocapture ts_perf`.
#[test]
#[ignore]
fn ts_perf() {
    let Some((_tmp, pkg)) = compiled() else { return };
    let (ok, text) = run(Command::new("node").args(["--expose-gc"]).arg(in_pkg(&pkg, "watch-tests/perf.js")).current_dir(&pkg));
    assert!(ok, "{text}");
    println!("perf ts: {text}");
    let v: serde_json::Value = serde_json::from_str(text.trim().lines().last().unwrap()).unwrap();
    assert!(v["overhead_us_per_item"].as_f64().unwrap() <= 5.0, "≤ 5 µs per item: {v}");
}
