//! RETURNS-17 · a generated streaming method returns `MethodStream<'ns.m', E>`
//! — a stream that carries its method's path — and a failed call throws a
//! typed `RemoteCallError`, on main's IR (2.x).
//!
//! 1. `rpc.ts` (the runtime: `MethodStream`, `RemoteCallError`, `RemoteError`,
//!    `Result`), `watch.ts` (its source type takes a `MethodStream`) and
//!    `transport.ts` (`TransportError`) match their goldens.
//! 2. Everything else is what main@3485a47 generated
//!    (`golden/returns17/before.sha256`), but for `client.ts`, whose only
//!    change is each streaming method's type and body and the helper import:
//!    mapped back to main's spelling, it hashes as before.
//! 3. The TypeScript tests in `tests/ts_returns17/` — types under `tsc`
//!    (`@ts-expect-error` as assertions) and behaviour on a mock socket under
//!    `node --test` — run against a package generated from
//!    `fixtures/returns17_ir.json`. They use `tests/ts_watch`'s pinned
//!    toolchain; without Node they are skipped unless
//!    `HUB_CODEGEN_REQUIRE_TS=1`.

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

/// sha256 of a file's blocks (split on blank lines), sorted — as RETURNS-38's
/// test hashes them (main's Rust generator orders blocks by `HashMap`).
fn sha256(s: &str) -> String {
    let mut blocks: Vec<&str> = s.split("\n\n").collect();
    blocks.sort();
    Sha256::digest(blocks.join("\n\n").as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

const FIXTURES: [&str; 3] = ["returns17_ir", "returns38_watch_ir", "synapse_cc_2_injected_ir"];

/// Every generated file, `<fixture>-<target>/<path>` → content.
fn outputs() -> BTreeMap<String, String> {
    let mut all = BTreeMap::new();
    for fx in FIXTURES {
        let ir = fixture(fx);
        for (name, t) in [("ts-ws", TransportEnv::Ws), ("ts-browser", TransportEnv::Browser), ("ts-none", TransportEnv::None)] {
            for (p, c) in ts(&ir, t) {
                all.insert(format!("{fx}-{name}/{p}"), c);
            }
        }
        for (p, c) in generate_rust(&ir).expect("rust").files {
            all.insert(format!("{fx}-rust/{p}"), c);
        }
    }
    all
}

// ─────────────────────────────────────────────────────────────
// 1. The runtime files
// ─────────────────────────────────────────────────────────────

fn golden(rel: &str, actual: &str) {
    let path = root().join("tests/golden").join(rel);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("golden {rel}"));
    assert_eq!(actual, expected, "{rel} diverged from its golden (UPDATE_GOLDEN=1 to accept)");
}

#[test]
fn rpc_ts_golden() {
    let files = ts(&fixture("returns17_ir"), TransportEnv::Browser);
    golden("returns17/rpc.ts", &files["rpc.ts"]);
    // the same whatever the IR and the transport
    for fx in FIXTURES {
        for t in [TransportEnv::Ws, TransportEnv::Browser, TransportEnv::None] {
            assert_eq!(ts(&fixture(fx), t)["rpc.ts"], files["rpc.ts"], "{fx} {t:?}");
        }
    }
}

#[test]
fn generated_streaming_methods_return_method_stream() {
    let files = ts(&fixture("returns17_ir"), TransportEnv::Browser);
    let alpha = &files["alpha/client.ts"];
    assert!(alpha.contains("import { MethodStream, collectOne } from '../rpc';"), "{alpha}");
    assert!(alpha.contains("  a(): MethodStream<'alpha.a', AlphaEvent>;"), "{alpha}");
    assert!(alpha.contains("  a(): MethodStream<'alpha.a', AlphaEvent> {"), "{alpha}");
    assert!(alpha.contains("    return new MethodStream<'alpha.a', AlphaEvent>('alpha.a', stream);"), "{alpha}");
    // a one-item method is what it was
    assert!(alpha.contains("  get(id: string): Promise<AlphaEvent>;"), "{alpha}");
    assert!(alpha.contains("    return collectOne<AlphaEvent>(stream);"), "{alpha}");
    assert!(!alpha.contains("AsyncGenerator") && !alpha.contains("extractData"), "{alpha}");
}

#[test]
fn transport_declares_transport_error() {
    for t in [TransportEnv::Ws, TransportEnv::Browser] {
        let tr = &ts(&fixture("returns17_ir"), t)["transport.ts"];
        assert!(tr.contains("export class TransportError extends Error"), "{t:?}");
        // the kit lifts transport.ts alone: no new import
        let imports: Vec<&str> = tr.lines().filter(|l| l.starts_with("import ")).collect();
        let expected: Vec<&str> = if t == TransportEnv::Ws {
            vec!["import WebSocket from 'ws';", "import type {", "import type { RpcClient } from './rpc';", "import { currentCallScope } from './watch';"]
        } else {
            vec!["import type {", "import type { RpcClient } from './rpc';", "import { currentCallScope } from './watch';"]
        };
        assert_eq!(imports, expected, "{t:?}");
    }
}

// ─────────────────────────────────────────────────────────────
// 2. Everything else unchanged
// ─────────────────────────────────────────────────────────────

const BEFORE: &str = "tests/golden/returns17/before.sha256";

fn before() -> BTreeMap<String, String> {
    std::fs::read_to_string(root().join(BEFORE))
        .expect("before.sha256")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (hash, path) = l.split_once("  ").expect("sha256sum line");
            (path.to_string(), hash.to_string())
        })
        .collect()
}

/// A `client.ts` as main spelled it: each streaming method's
/// `MethodStream<'path', T>` back to `AsyncGenerator<T>`, its body back to
/// `yield* extractData<T>(stream)`, the helper import back to `extractData`.
/// Anything else RETURNS-17 changed would survive this and fail the hash.
fn to_main_spelling(client: &str) -> String {
    let mut out = Vec::new();
    for line in client.split('\n') {
        let mapped = if line.starts_with("import { MethodStream") && line.ends_with("rpc';") {
            line.replacen("MethodStream", "extractData", 1)
        } else if let Some((head, rest)) = line.split_once("MethodStream<'") {
            // `  name(params): MethodStream<'p', T>;` / `… {`, or the body's `return new MethodStream<'p', T>('p', stream);`
            let (_, t_and_tail) = rest.split_once("', ").expect("MethodStream<'path', T>");
            if head.trim_start() == "return new " {
                let t = t_and_tail.rsplit_once(">('").expect("the constructor").0;
                format!("    yield* extractData<{t}>(stream);")
            } else if let Some(t) = t_and_tail.strip_suffix("> {") {
                let indent = &head[..head.len() - head.trim_start().len()];
                format!("{indent}async *{}AsyncGenerator<{t}> {{", head.trim_start())
            } else {
                let t = t_and_tail.strip_suffix(">;").expect("an interface line");
                format!("{head}AsyncGenerator<{t}>;")
            }
        } else {
            line.to_string()
        };
        out.push(mapped);
    }
    out.join("\n")
}

#[test]
fn existing_output_unchanged() {
    let now = outputs();
    if std::env::var("UPDATE_BEFORE").is_ok() {
        let mut text = String::from("# RETURNS-17: every file main@3485a47 generated for the three fixtures (sha256 of its sorted blocks)\n");
        for (p, c) in &now {
            text.push_str(&format!("{}  {}\n", sha256(c), p));
        }
        std::fs::write(root().join(BEFORE), text).expect("write before");
        return;
    }
    let before = before();
    let mut compared = 0;
    for (path, hash) in &before {
        let file = path.rsplit_once('/').map(|(_, f)| f).unwrap_or(path);
        let name = path.split_once('/').unwrap().1;
        // the runtime files have goldens of their own (section 1); the
        // package's version and the metadata are hashes of the content
        if ["rpc.ts", "watch.ts", "transport.ts", "package.json", ".codegen-metadata.json"].contains(&name) {
            continue;
        }
        let content = now.get(path).unwrap_or_else(|| panic!("{path} is no longer generated"));
        let content = if file == "client.ts" && !path.contains("-rust/") { to_main_spelling(content) } else { content.clone() };
        assert_eq!(&sha256(&content), hash, "{path} changed beyond RETURNS-17's spelling");
        compared += 1;
    }
    for path in now.keys() {
        assert!(before.contains_key(path), "{path} is new");
    }
    assert!(compared > 90, "compared {compared} files");
}

/// The mapping is not vacuous: it changes what RETURNS-17 changed, and only that.
#[test]
fn mapping_back_is_exact() {
    let client = &ts(&fixture("returns17_ir"), TransportEnv::Browser)["alpha/client.ts"];
    let main = to_main_spelling(client);
    assert!(main.contains("  a(): AsyncGenerator<AlphaEvent>;"), "{main}");
    assert!(main.contains("  async *a(): AsyncGenerator<AlphaEvent> {"), "{main}");
    assert!(main.contains("    yield* extractData<AlphaEvent>(stream);"), "{main}");
    assert!(main.contains("import { extractData, collectOne } from '../rpc';"), "{main}");
    assert!(!main.contains("MethodStream"), "{main}");
}

// ─────────────────────────────────────────────────────────────
// 3. The TypeScript tests
// ─────────────────────────────────────────────────────────────

fn run(cmd: &mut Command) -> (bool, String) {
    let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// A generated package with the tests beside it, compiled to CommonJS by
/// `tsc` (which also type-checks `types.test.ts`).
fn compiled() -> Option<(tempfile::TempDir, PathBuf)> {
    let node_ok = Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !node_ok {
        assert!(std::env::var("HUB_CODEGEN_REQUIRE_TS").is_err(), "HUB_CODEGEN_REQUIRE_TS=1 and there is no node");
        eprintln!("node not available; skipping the TypeScript RETURNS-17 tests");
        return None;
    }
    let toolchain = root().join("tests/ts_watch");
    if !toolchain.join("node_modules/.bin/tsc").exists() {
        let (ok, text) = run(Command::new("npm").args(["ci", "--no-audit", "--no-fund"]).current_dir(&toolchain));
        assert!(ok, "npm ci in {}: {text}", toolchain.display());
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = tmp.path().join("pkg");
    for (path, content) in ts(&fixture("returns17_ir"), TransportEnv::Browser) {
        let p = pkg.join(&path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    let tests = pkg.join("r17-tests");
    std::fs::create_dir_all(&tests).unwrap();
    for f in ["types.test.ts", "runtime.test.ts", "perf.ts"] {
        std::fs::copy(root().join("tests/ts_returns17").join(f), tests.join(f)).unwrap();
    }
    std::os::unix::fs::symlink(toolchain.join("node_modules"), pkg.join("node_modules")).unwrap();
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
  "include": ["*.ts", "alpha/**/*.ts", "beta/**/*.ts", "gamma/**/*.ts", "r17-tests/*.ts"]
}"#;
    std::fs::write(pkg.join("tsconfig.r17.json"), tsconfig).unwrap();
    let (ok, text) = run(Command::new(toolchain.join("node_modules/.bin/tsc")).args(["-p", "tsconfig.r17.json"]).current_dir(&pkg));
    assert!(ok, "tsc rejected the generated package or the RETURNS-17 tests:\n{text}");
    std::fs::write(pkg.join("out/package.json"), r#"{ "type": "commonjs" }"#).unwrap();
    Some((tmp, pkg))
}

fn in_pkg(pkg: &Path, rel: &str) -> PathBuf {
    pkg.join("out").join(rel)
}

#[test]
fn ts_method_stream_types_and_behaviour() {
    let Some((_tmp, pkg)) = compiled() else { return };
    let (ok, text) = run(Command::new("node").args(["--test", "--test-reporter=spec"]).arg(in_pkg(&pkg, "r17-tests/runtime.test.js")).current_dir(&pkg));
    println!("{text}");
    assert!(ok, "node --test runtime.test.js failed:\n{text}");
    assert!(text.contains("pass 11"), "11 runtime tests:\n{text}");
}

/// Every `@ts-expect-error` is real: unmarked, `tsc` fails on exactly those lines.
#[test]
fn ts_expected_errors_are_errors() {
    let Some((_tmp, pkg)) = compiled() else { return };
    let types = pkg.join("r17-tests/types.test.ts");
    let src = std::fs::read_to_string(&types).unwrap();
    let marked = src.matches("// @ts-expect-error").count();
    assert!(marked >= 8, "{marked} expected errors");
    std::fs::write(&types, src.replace("// @ts-expect-error", "// (unmarked)")).unwrap();
    let toolchain = root().join("tests/ts_watch");
    let (ok, text) = run(Command::new(toolchain.join("node_modules/.bin/tsc")).args(["-p", "tsconfig.r17.json", "--noEmit"]).current_dir(&pkg));
    assert!(!ok, "unmarked, the type tests must fail");
    let errors = text.lines().filter(|l| l.contains("r17-tests/types.test.ts") && l.contains("error TS")).count();
    assert_eq!(errors, marked, "one error per marked line:\n{text}");
}

/// Perf: `MethodStream`'s per-item overhead over the bare generator it
/// replaced. `cargo test -- --ignored perf` prints it.
#[test]
#[ignore]
fn perf_method_stream_overhead() {
    let Some((_tmp, pkg)) = compiled() else { return };
    let (ok, text) = run(Command::new("node").arg(in_pkg(&pkg, "r17-tests/perf.js")).current_dir(&pkg));
    println!("{text}");
    assert!(ok, "{text}");
}
