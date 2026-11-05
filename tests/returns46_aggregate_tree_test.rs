//! RETURNS-46 · the TypeScript aggregate (`{Ns}Event`, in `<ns>/events.ts`)
//! nests a static child's aggregate, and a dynamic child's under the
//! instance's name — the wire shape the Rust aggregate has, from the IR.
//!
//! The IR is `fixtures/returns46_tree_ir.json`, a copy of plexus-macros'
//! `fixtures/aggregate-tree/expected/ir.json` (as its service writes it),
//! but for seven doc lines of plexus-core's own protocol types (`PluginSchema`,
//! `MethodRole`, …) that still use the retired word for a service, spelled
//! *service* here for the naming gate — renaming them at the source moves
//! every hash of the conformance fixture, a contract change of its own;
//! `fixtures/returns46_aggregate.<level>.jsonl` are copies of that
//! fixture's `expected/aggregate.<level>.jsonl`. Under `tsc`:
//!
//! - every line of each level's JSON is accepted as that level's aggregate
//!   (`lines.ts`, written here from the files);
//! - `ts_returns46/types.test.ts`: the variants exactly, wrong nestings
//!   refused (`@ts-expect-error`, each one real), an exhaustive `switch`.
//!
//! The dynamic child's event is `unknown`: IR 3.0 names the dynamic child
//! (`tree.item`, role `dynamic_child`) but describes none of its methods —
//! a service lists only its static children's schemas.
//!
//! They use `tests/ts_watch`'s pinned toolchain; without Node they are
//! skipped unless `SERVICE_CODEGEN_REQUIRE_TS=1`.

use service_codegen::generator::{GenerationOptions, TransportEnv};
use service_codegen::{generate_typescript, IR};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ir() -> IR {
    let path = root().join("tests/fixtures/returns46_tree_ir.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("read fixture")).expect("the fixture decodes")
}

fn ts(ir: &IR) -> HashMap<String, String> {
    generate_typescript(ir, &GenerationOptions { transport: TransportEnv::Browser, ..GenerationOptions::default() })
        .expect("generate typescript")
        .files
}

fn jsonl(level: &str) -> Vec<String> {
    let path = root().join(format!("tests/fixtures/returns46_aggregate.{level}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_namespace_gets_its_aggregate() {
    let files = ts(&ir());
    let tree = &files["tree/events.ts"];
    assert!(tree.contains("export type TreeEvent ="), "{tree}");
    assert!(tree.contains("  | { method: 'branch'; event: TreeBranchEvent }"), "{tree}");
    assert!(tree.contains("  | { method: 'item'; event: { name: string; event: unknown } }"), "{tree}");
    assert!(tree.contains("import type { TreeBranchEvent } from '../tree/branch/events';"), "{tree}");
    let branch = &files["tree/branch/events.ts"];
    assert!(branch.contains("  | { method: 'grow'; event: Result<Bud, GrowError> }"), "{branch}");
    assert!(branch.contains("  | { method: 'leaf'; event: TreeBranchLeafEvent }"), "{branch}");
    assert!(files["tree/branch/leaf/events.ts"].contains("  | { method: 'fall'; event: Fell }"));
    for ns in ["tree", "tree/branch", "tree/branch/leaf"] {
        assert!(files[&format!("{ns}/index.ts")].contains("export * from './events';"), "{ns}");
    }
}

fn run(cmd: &mut Command) -> (bool, String) {
    let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// A generated package with the tests beside it, and the toolchain's `tsc`.
fn package() -> Option<(tempfile::TempDir, PathBuf, PathBuf)> {
    let node_ok = Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !node_ok {
        assert!(std::env::var("SERVICE_CODEGEN_REQUIRE_TS").is_err(), "SERVICE_CODEGEN_REQUIRE_TS=1 and there is no node");
        eprintln!("node not available; skipping the TypeScript RETURNS-46 tests");
        return None;
    }
    let toolchain = root().join("tests/ts_watch");
    if !toolchain.join("node_modules/.bin/tsc").exists() {
        let (ok, text) = run(Command::new("npm").args(["ci", "--no-audit", "--no-fund"]).current_dir(&toolchain));
        assert!(ok, "npm ci in {}: {text}", toolchain.display());
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = tmp.path().join("pkg");
    for (path, content) in ts(&ir()) {
        let p = pkg.join(&path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    let tests = pkg.join("r46-tests");
    std::fs::create_dir_all(&tests).unwrap();
    for f in ["types.test.ts", "runtime.test.ts"] {
        std::fs::copy(root().join("tests/ts_returns46").join(f), tests.join(f)).unwrap();
    }
    let mut lines = String::from(
        "// RETURNS-46 · the aggregate-tree fixture's JSON, each line typed as its level's aggregate.\n\
         import type { TreeEvent } from '../tree/events';\n\
         import type { TreeBranchEvent } from '../tree/branch/events';\n\
         import type { TreeBranchLeafEvent } from '../tree/branch/leaf/events';\n",
    );
    for (level, ty, name) in
        [("tree", "TreeEvent", "tree"), ("tree.branch", "TreeBranchEvent", "branch"), ("tree.branch.leaf", "TreeBranchLeafEvent", "leaf")]
    {
        lines.push_str(&format!("export const {name}: {ty}[] = [\n"));
        for l in jsonl(level) {
            lines.push_str(&format!("  {l},\n"));
        }
        lines.push_str("];\n");
    }
    std::fs::write(tests.join("lines.ts"), lines).unwrap();
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
  "include": ["*.ts", "tree/**/*.ts", "r46-tests/*.ts"]
}"#;
    std::fs::write(pkg.join("tsconfig.r46.json"), tsconfig).unwrap();
    let tsc = toolchain.join("node_modules/.bin/tsc");
    Some((tmp, pkg, tsc))
}

fn tsc(pkg: &Path, tsc: &Path) -> (bool, String) {
    run(Command::new(tsc).args(["-p", "tsconfig.r46.json", "--noEmit"]).current_dir(pkg))
}

/// RETURNS-46 · at run time: `.events()` through a child accessor, and
/// `watch` over such calls, yield the nested aggregate (node --test).
#[test]
fn ts_lifted_events_at_run_time() {
    let Some((_tmp, pkg, bin)) = package() else { return };
    let (ok, text) = run(Command::new(&bin).args(["-p", "tsconfig.r46.json"]).current_dir(&pkg));
    assert!(ok, "tsc:\n{text}");
    std::fs::write(pkg.join("out/package.json"), r#"{ "type": "commonjs" }"#).unwrap();
    let (ok, text) = run(Command::new("node").args(["--test", "--test-reporter=spec"]).arg(pkg.join("out/r46-tests/runtime.test.js")).current_dir(&pkg));
    println!("{text}");
    assert!(ok, "node --test runtime.test.js failed:\n{text}");
    assert!(text.contains("pass 5"), "5 runtime tests:\n{text}");
}

#[test]
fn ts_the_aggregate_accepts_the_fixtures_json_and_its_type_tests() {
    let Some((_tmp, pkg, bin)) = package() else { return };
    let (ok, text) = tsc(&pkg, &bin);
    assert!(ok, "tsc rejected the generated package, the fixture's JSON or the RETURNS-46 type tests:\n{text}");
}

/// Every `@ts-expect-error` is real: unmarked, `tsc` fails on exactly those lines.
#[test]
fn ts_wrong_nestings_are_errors() {
    let Some((_tmp, pkg, bin)) = package() else { return };
    let types = pkg.join("r46-tests/types.test.ts");
    let src = std::fs::read_to_string(&types).unwrap();
    let marked = src.matches("// @ts-expect-error").count();
    assert!(marked >= 6, "{marked} expected errors");
    std::fs::write(&types, src.replace("// @ts-expect-error", "// (unmarked)")).unwrap();
    let (ok, text) = tsc(&pkg, &bin);
    assert!(!ok, "unmarked, the type tests must fail");
    // The line under each marker (1-based), and the lines tsc names: the
    // same set — a wrong nesting may draw more than one error on its line.
    let expected: std::collections::BTreeSet<usize> =
        src.lines().enumerate().filter(|(_, l)| l.contains("// @ts-expect-error")).map(|(i, _)| i + 2).collect();
    let errors: std::collections::BTreeSet<usize> = text
        .lines()
        .filter(|l| l.contains("error TS"))
        .filter_map(|l| l.strip_prefix("r46-tests/types.test.ts("))
        .filter_map(|l| l.split(',').next()?.parse().ok())
        .collect();
    assert_eq!(errors, expected, "an error on each marked line, and nowhere else:\n{text}");
}
