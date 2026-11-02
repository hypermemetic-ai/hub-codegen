// Shared by the tests that hold today's output to a hash record of an older
// main (returns17/before.sha256, returns38/before.sha256). Those records were
// taken before RETURNS-36 brought the returns/0.7 line in, and two of its
// changes reach the recorded files on purpose:
//
// - RETURNS-22: a generated smoke test has no `ws://localhost:4444` default;
//   with no `PLEXUS_URL` it throws.
// - RETURNS-16: a tagged Rust enum renames each variant to its exact wire tag
//   (`#[serde(rename = "status_changed")]`) instead of `rename_all =
//   "lowercase"`, which spelled `status_changed` as `statuschanged`.
//
// The output is spelled back as the records had it before it is hashed;
// anything else that moved still fails the hash. Each change has its own
// test that holds the new form (tests/returns16_variant_tags_test.rs,
// typescript_codegen_test.rs's backend-URL cases).

const SMOKE_NOW: &str = "const WS_URL = process.env.PLEXUS_URL ?? \"\";\nif (!WS_URL) throw new Error(\"PLEXUS_URL is not set: export the backend's WebSocket URL (ws://host:port)\");\n";
const SMOKE_BEFORE: &str = "const WS_URL = process.env.PLEXUS_URL ?? \"ws://localhost:4444\";\n";

/// Today's output, as the records before RETURNS-36 had it.
pub fn before_returns_line(s: &str) -> String {
    let s = s.replace(SMOKE_NOW, SMOKE_BEFORE);
    let lines: Vec<&str> = s.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut in_tagged_enum = false;
    for (i, line) in lines.iter().enumerate() {
        let next = lines.get(i + 1).copied().unwrap_or("");
        if let Some(tag) = line.strip_prefix("#[serde(tag = \"").and_then(|r| r.strip_suffix("\")]")) {
            if !tag.contains('"') && next.starts_with("pub enum ") {
                out.push(format!("#[serde(tag = \"{tag}\", rename_all = \"lowercase\")]"));
                continue;
            }
        }
        if line.starts_with("pub enum ") {
            in_tagged_enum = i > 0 && lines[i - 1].starts_with("#[serde(tag = \"");
        } else if *line == "}" {
            in_tagged_enum = false;
        } else if in_tagged_enum
            && line.starts_with("    #[serde(rename = \"")
            && line.ends_with("\")]")
        {
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}
