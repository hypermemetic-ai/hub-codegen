// Shared by the tests that hold today's output to a hash record of an older
// main (returns17/before.sha256, returns38/before.sha256). NAMING-10 renamed the
// generator and the words in its fixtures; those records predate it, so the
// output is spelled back the old way before it is hashed. Nothing else moves.
// The old spellings sit on `naming-alias` lines; NAMING-18 re-records the
// hashes and removes this file.

/// Today's output of the file at `path` (`<fixture>-<target>/…`) in the
/// spelling the records were taken in.
pub fn as_recorded(path: &str, s: &str) -> String {
    let now = format!("service-codegen v{}", env!("CARGO_PKG_VERSION"));
    let s = s.replace(&now, "hub-codegen v0.5.0") // naming-alias: the header before NAMING-10
        .replace("service-codegen", "hub-codegen") // naming-alias: the tool's name before NAMING-10
        .replace("A gated service (AUTH-2)", "A gated hub (AUTH-2)") // naming-alias: a comment's word before NAMING-10
        .replace("(DynamicService routing layer)", "(DynamicHub routing layer)"); // naming-alias: a fixture's description before NAMING-10
    if !path.starts_with("returns38_watch_ir-") {
        return s;
    }
    // returns38_watch_ir's backend was named for the old word; it is `service` now
    s.replace(" * Backend: service\n", " * Backend: hub\n") // naming-alias: the fixture's backend before NAMING-10
        .replace("smoke test for service backend", "smoke test for hub backend") // naming-alias: the fixture's backend before NAMING-10
        .replace("backend: \"service\",", "backend: \"hub\",") // naming-alias: the fixture's backend before NAMING-10
        .replace("connects to service backend", "connects to hub backend") // naming-alias: the fixture's backend before NAMING-10
        .replace("\"service.schema", "\"hub.schema") // naming-alias: the fixture's backend before NAMING-10
}
