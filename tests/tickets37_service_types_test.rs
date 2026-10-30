//! TICKETS-37 · a type the service declares, used by an activation, is
//! generated and imported.
//!
//! synapse ≤ 3.16 (IR 2.0) keeps one copy of a type that several schemas
//! declare, and puts it in the protocol root's namespace (`""`) whenever the
//! root declares it too: since SYNAPSE-CC-9 every activation's `schema`
//! returns `SchemaResult`, whose `$defs` hold `Scope` and `MethodPath`, so
//! menger's IR names `.Scope` (from `members.MembersEvent`, `orgs.Grant`)
//! and `.MethodPath` (from `auth.requires`). service-codegen 0.5.1 wrote
//! those references by their local names, generated no root type and
//! imported none: `tsc` refused the client with TS2304.
//!
//! A service-level type a generated namespace uses now lives in one of the
//! namespaces that use it — the one whose methods and types refer to it
//! most, a tie to the shortest name, then the alphabetically first (IR 2.1's
//! rule, SYNAPSE-CC-8, which never chooses the root) — and every other
//! namespace imports it from there. The types it refers to follow it. The
//! protocol root's own types stay ungenerated.
//!
//! The fixture, `fixtures/tickets37_service_types_ir.json`, is that shape in
//! small: the root's `Scope`, `MethodPath`, `Origin`, `Grantee` and
//! `GranteeKind` used from `auth`, `members`, `members.teams` and `orgs`,
//! and `PluginSchema` and `RequiredCredential` used only by the root.

use service_codegen::generator::{GenerationOptions, TransportEnv};
use service_codegen::{generate_typescript, IR};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture() -> IR {
    let path = root().join("tests/fixtures/tickets37_service_types_ir.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("read fixture")).expect("the fixture decodes")
}

fn ts_with(options: GenerationOptions) -> HashMap<String, String> {
    generate_typescript(&fixture(), &options).expect("generate typescript").files
}

fn ts() -> HashMap<String, String> {
    ts_with(GenerationOptions { transport: TransportEnv::Browser, ..GenerationOptions::default() })
}

fn file<'a>(files: &'a HashMap<String, String>, path: &str) -> &'a str {
    files.get(path).unwrap_or_else(|| {
        let mut have: Vec<&String> = files.keys().collect();
        have.sort();
        panic!("{path} was not generated; the files are {have:?}")
    })
}

/// The files that declare `export … name` (a type alias, an interface or a
/// union).
fn declared_in(files: &HashMap<String, String>, name: &str) -> Vec<String> {
    let alias = format!("export type {name} =");
    let interface = format!("export interface {name} ");
    let mut at: Vec<String> = files
        .iter()
        .filter(|(_, s)| s.contains(&alias) || s.contains(&interface))
        .map(|(p, _)| p.clone())
        .collect();
    at.sort();
    at
}

#[test]
fn a_service_level_type_lives_in_the_namespace_that_uses_it_most() {
    let files = ts();
    // members refers to Scope twice (granted, revoked); members.teams and orgs once each
    assert_eq!(declared_in(&files, "Scope"), ["members/types.ts"]);
    // only auth refers to it
    assert_eq!(declared_in(&files, "MethodPath"), ["auth/types.ts"]);
    // auth and orgs once each: the shorter name, then the first — auth
    assert_eq!(declared_in(&files, "Origin"), ["auth/types.ts"]);
    // orgs alone; GranteeKind follows Grantee, the type that names it
    assert_eq!(declared_in(&files, "Grantee"), ["orgs/types.ts"]);
    assert_eq!(declared_in(&files, "GranteeKind"), ["orgs/types.ts"]);
    // as the IR describes them
    assert!(file(&files, "members/types.ts").contains("export type Scope = string;"));
    assert!(file(&files, "auth/types.ts").contains("export type MethodPath = string;"));
    assert!(file(&files, "orgs/types.ts").contains(r#"export type GranteeKind = "principal" | "team" | "everyone";"#));
}

#[test]
fn every_reference_to_a_service_level_type_is_imported() {
    let files = ts();
    // a method's parameter, in its own namespace's client
    let auth = file(&files, "auth/client.ts");
    assert!(auth.contains("import type { AuthEvent, MethodPath, Origin } from './types';"), "{auth}");
    assert!(auth.contains("requires(method: MethodPath)"), "{auth}");
    // a nested namespace's method, from its parent's module
    let teams = file(&files, "members/teams/client.ts");
    assert!(teams.contains("import type { MembersEvent, Scope } from '../../members/types';"), "{teams}");
    // a type's fields, from two other activations — the one that holds the
    // service-level types, and the one whose own type it always imported
    let orgs = file(&files, "orgs/types.ts");
    assert!(orgs.contains("import type { Origin } from '../auth/types';"), "{orgs}");
    assert!(orgs.contains("import type { Principal, Scope } from '../members/types';"), "{orgs}");
    // the home imports nothing of its own
    let members = file(&files, "members/types.ts");
    assert!(!members.contains("import"), "{members}");
}

#[test]
fn the_protocol_roots_own_types_stay_ungenerated() {
    let files = ts();
    for name in ["PluginSchema", "RequiredCredential"] {
        assert!(declared_in(&files, name).is_empty(), "{name} is the root's alone: {:?}", declared_in(&files, name));
        for (path, s) in &files {
            assert!(!s.contains(name), "{path} names {name}");
        }
    }
    // the transport's types are the protocol's, as before: none of the
    // service-level types is declared or imported there
    let types = file(&files, "types.ts");
    for name in ["Scope", "MethodPath", "Origin", "Grantee", "GranteeKind"] {
        for decl in [format!("export type {name} ="), format!("export interface {name} "), format!(" {name} }} from")] {
            assert!(!types.contains(&decl), "types.ts has `{decl}`");
        }
    }
}

#[test]
fn a_plugin_filter_takes_in_the_home_of_a_service_level_type() {
    let files = ts_with(GenerationOptions {
        transport: TransportEnv::Browser,
        plugins_filter: Some(vec!["members.teams".to_string()]),
        ..GenerationOptions::default()
    });
    // members.teams is requested; members holds MembersEvent and Scope, so its types come too
    assert!(files.contains_key("members/teams/client.ts"));
    assert!(file(&files, "members/types.ts").contains("export type Scope = string;"));
    assert!(!files.contains_key("orgs/types.ts"), "orgs is not needed");
}

// ─────────────────────────────────────────────────────────────
// The generated package under tsc (tests/ts_watch's pinned toolchain)
// ─────────────────────────────────────────────────────────────

fn run(cmd: &mut Command) -> (bool, String) {
    let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

#[test]
fn the_generated_package_type_checks() {
    let node_ok = Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !node_ok {
        assert!(std::env::var("SERVICE_CODEGEN_REQUIRE_TS").is_err(), "SERVICE_CODEGEN_REQUIRE_TS=1 and there is no node");
        eprintln!("node not available; skipping the TICKETS-37 type check");
        return;
    }
    let toolchain = root().join("tests/ts_watch");
    if !toolchain.join("node_modules/.bin/tsc").exists() {
        let (ok, text) = run(Command::new("npm").args(["ci", "--no-audit", "--no-fund"]).current_dir(&toolchain));
        assert!(ok, "npm ci in {}: {text}", toolchain.display());
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let pkg = tmp.path().join("pkg");
    for (path, content) in ts() {
        let p = pkg.join(&path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    std::os::unix::fs::symlink(toolchain.join("node_modules"), pkg.join("node_modules")).unwrap();
    let tsconfig = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022", "DOM"],
    "types": [],
    "strict": true,
    "skipLibCheck": true,
    "noEmit": true
  },
  "include": ["*.ts", "auth/**/*.ts", "members/**/*.ts", "orgs/**/*.ts"]
}"#;
    std::fs::write(pkg.join("tsconfig.t37.json"), tsconfig).unwrap();
    let (ok, text) = run(Command::new(toolchain.join("node_modules/.bin/tsc")).args(["-p", "tsconfig.t37.json"]).current_dir(&pkg));
    assert!(ok, "tsc refused the generated package:\n{text}");
}
