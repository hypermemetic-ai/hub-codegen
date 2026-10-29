//! NAMING-10: the JSON that passes between synapse-cc and service-codegen says
//! service. Each reader takes the old key and the new; each writer writes only
//! the new. The old spellings live only on lines marked `naming-alias`, which
//! NAMING-18 removes.

use std::path::PathBuf;
use std::process::Command;

use service_codegen::cache::{
    get_cache_dir_under, read_cache_manifest_under, write_cache_manifest_to, CodeCacheManifest,
    ToolchainVersions,
};

const OLD_TOOL: &str = "hub-codegen"; // naming-alias: the cache key and directory before NAMING-10
const OLD_OUTPUT_KEY: &str = "hubCodegenVersion"; // naming-alias: the output key before NAMING-10

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/returns38_watch_ir.json")
}

fn manifest_json(tool_key: &str) -> String {
    format!(
        r#"{{"version":"2.0","target":"typescript","toolchain":{{"synapse-cc":"0.3.5","synapse":"3.16.0","{tool_key}":"0.5.0"}},"updatedAt":"2026-10-29T00:00:00Z","plugins":{{}}}}"#
    )
}

#[test]
fn output_json_says_service_codegen_version_only() {
    let out = Command::new(env!("CARGO_BIN_EXE_service-codegen"))
        .arg(fixture())
        .args(["--output-format", "json"])
        .output()
        .expect("run service-codegen");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["serviceCodegenVersion"], env!("CARGO_PKG_VERSION"));
    assert!(v.get(OLD_OUTPUT_KEY).is_none(), "the old output key is not written");
}

#[test]
fn a_cache_manifest_written_with_the_old_key_reads() {
    let m: CodeCacheManifest = serde_json::from_str(&manifest_json(OLD_TOOL)).expect("old key reads");
    assert_eq!(m.toolchain.service_codegen, "0.5.0");
    let m: CodeCacheManifest = serde_json::from_str(&manifest_json("service-codegen")).expect("new key reads");
    assert_eq!(m.toolchain.service_codegen, "0.5.0");
}

#[test]
fn a_cache_manifest_is_written_with_the_new_key_only() {
    let m = CodeCacheManifest::new(
        "typescript".into(),
        ToolchainVersions { synapse_cc: "0.3.6".into(), synapse: "3.16.0".into(), service_codegen: "0.5.1".into() },
    );
    let s = serde_json::to_string(&m).unwrap();
    assert!(s.contains(r#""service-codegen":"0.5.1""#), "{s}");
    assert!(!s.contains(OLD_TOOL), "{s}");
}

#[test]
fn the_cache_lives_under_service_codegen_and_an_old_one_still_reads() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().to_path_buf();
    let new_dir = get_cache_dir_under(&root, "typescript", "menger");
    assert!(new_dir.ends_with(".cache/plexus-codegen/service-codegen/typescript/menger"), "{}", new_dir.display());

    // a cache left by the old name is read when there is no new one
    let old_dir = root.join(".cache/plexus-codegen").join(OLD_TOOL).join("typescript/menger");
    std::fs::create_dir_all(&old_dir).unwrap();
    std::fs::write(old_dir.join("manifest.json"), manifest_json(OLD_TOOL)).unwrap();
    let m = read_cache_manifest_under(&root, "typescript", "menger").expect("the old cache reads");
    assert_eq!(m.toolchain.service_codegen, "0.5.0");

    // once a new one is written, it wins
    let mut fresh = m.clone();
    fresh.toolchain.service_codegen = "0.5.1".into();
    write_cache_manifest_to(&new_dir, &fresh).unwrap();
    let m = read_cache_manifest_under(&root, "typescript", "menger").unwrap();
    assert_eq!(m.toolchain.service_codegen, "0.5.1");
}
