//! The naming gate (owner, 2026-10-29: a Plexus backend is a service, never
//! the old word; ~/AGENTS.md "Naming", NAMING-10): `scripts/naming-gate.sh`
//! fails on any remnant of the old word in the repository, so a new one fails
//! `cargo test`. Its allowlist and the `naming-alias` marker are in the script.

use std::process::Command;

#[test]
fn no_remnant_of_the_old_word() {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new("bash")
        .arg(format!("{root}/scripts/naming-gate.sh"))
        .current_dir(root)
        .output()
        .expect("run scripts/naming-gate.sh");
    assert!(
        out.status.success(),
        "scripts/naming-gate.sh found lines to rename:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
