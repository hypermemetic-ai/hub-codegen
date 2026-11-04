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
// - RETURNS-46: every namespace's aggregate, `{Ns}Event`, in a new
//   `<ns>/events.ts` beside its `client.ts`, which `<ns>/index.ts`
//   re-exports. The new files are not in the records
//   ([`is_returns46_events`]); [`before_returns46`] takes the export line
//   back out of a namespace's `index.ts`, and the lifting out of a client
//   (generic over the aggregate its events lift into);
//   tests/returns46_aggregate_tree_test.rs and the synapse_cc_2 goldens
//   hold the new form.
//
// The output is spelled back as the records had it before it is hashed;
// anything else that moved still fails the hash. Each change has its own
// test that holds the new form (tests/returns16_variant_tags_test.rs,
// typescript_codegen_test.rs's backend-URL cases).

const SMOKE_NOW: &str = "const WS_URL = process.env.PLEXUS_URL ?? \"\";\nif (!WS_URL) throw new Error(\"PLEXUS_URL is not set: export the backend's WebSocket URL (ws://host:port)\");\n";
const SMOKE_BEFORE: &str = "const WS_URL = process.env.PLEXUS_URL ?? \"ws://localhost:4444\";\n";

const EVENTS_EXPORT: &str = "export * from './events';\n";

/// RETURNS-46: a namespace's `events.ts` — beside a generated namespace's
/// `client.ts` in `files` — new since the records.
#[allow(dead_code)]
pub fn is_returns46_events(path: &str, files: &dyn Fn(&str) -> bool) -> bool {
    match path.strip_suffix("/events.ts") {
        Some(dir) => files(&format!("{dir}/client.ts")),
        None => false,
    }
}

/// RETURNS-46's output, as the records had it: a namespace's `index.ts`
/// (one beside a generated `events.ts`) without the line exporting it, and a
/// namespace `client.ts` without the lifting. Every other file as it is.
#[allow(dead_code)]
pub fn before_returns46(path: &str, s: &str, files: &dyn Fn(&str) -> bool) -> String {
    let dir = path.rsplit_once('/').map(|(d, _)| d);
    if path.ends_with("/index.ts") && dir.is_some_and(|d| files(&format!("{d}/events.ts"))) {
        return s.replacen(EVENTS_EXPORT, "", 1);
    }
    if path.ends_with("/client.ts") && !path.contains("-rust/") {
        return unlift_client(s);
    }
    s.to_string()
}

/// The lifting RETURNS-46 writes into a client in a static-child relation,
/// taken out line by line; a client with none is returned as it is.
fn unlift_client(s: &str) -> String {
    let generic = |l: &str, open: &str| -> Option<String> {
        // `XClient<anything>` → `XClient` at the first `open`
        let i = l.find(open)?;
        let start = i + open.len();
        let mut depth = 1;
        for (j, c) in l[start..].char_indices() {
            match c {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(format!("{}{}", &l[..i + open.len() - 1], &l[start + j + 1..]));
                    }
                }
                _ => {}
            }
        }
        None
    };
    let mut out: Vec<String> = Vec::new();
    let lines: Vec<&str> = s.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        i += 1;
        let t = l.trim_start();
        if (t.starts_with("import type {") && t.ends_with("} from './events';"))
            || (t == format!("import type {{ Up }} from '{}rpc';", t.split("from '").nth(1).map(|r| &r[..r.len() - "rpc';".len()]).unwrap_or("")))
            || t.starts_with("private readonly __lift:")
            || t == "this.__lift = lift;"
            || t == "const lift = this.__lift;"
        {
            continue;
        }
        if t.starts_with("private __liftUp(") {
            i += 3; // its body, its `}` and the blank line after it
            continue;
        }
        let mut l = l.to_string();
        if let Some(rest) = l.strip_prefix("export interface ").map(str::to_string) {
            if let Some(name) = rest.strip_suffix("<A = undefined> {") {
                l = format!("export interface {name} {{");
            }
        }
        if l.contains("ClientImpl<A = undefined> implements ") {
            l = l.replacen("<A = undefined>", "", 1);
            l = l.replacen("Client<A> {", "Client {", 1);
        }
        if l.trim_start().starts_with("readonly ") && l.contains("Client<") {
            if let Some(m) = generic(&l, "Client<") {
                l = m;
            }
        }
        if l.contains(" = create") && l.contains("Client<") && l.contains("(rpc, ") {
            let head = &l[..l.find("Client<").unwrap() + "Client".len()];
            l = format!("{head}(rpc);");
        }
        if l.contains("MethodStream<'") {
            if let Some(p) = l.find(", Up<A, ") {
                // `MethodStream<'p', T, Up<A, XEvent>>` → `MethodStream<'p', T>`
                if let Some(end) = generic(&l[p + 2..], "Up<") {
                    l = format!("{}{}", &l[..p], &end["Up".len()..]);
                }
            }
            if let Some(q) = l.find("', stream, (event: ") {
                l = format!("{}', stream);", &l[..q]);
            }
        }
        if l.starts_with("  constructor(rpc: RpcClient, lift?: ") {
            // a client with no children of its own had the one-line form
            let body_alone = lines.get(i).map(|n| n.trim()) == Some("this.rpc = rpc;")
                && lines.get(i + 1).map(|n| n.trim()) == Some("this.__lift = lift;")
                && lines.get(i + 2).map(|n| n.trim()) == Some("}");
            if body_alone {
                out.push("  constructor(rpc: RpcClient) { this.rpc = rpc; }".to_string());
                i += 3;
                continue;
            }
            l = "  constructor(rpc: RpcClient) {".to_string();
        }
        if l.starts_with("export function create") && l.contains("Client<A = undefined>(rpc: RpcClient, lift?: ") {
            let name = &l["export function create".len()..l.find("Client<A").unwrap()];
            l = format!("export function create{name}Client(rpc: RpcClient): {name}Client {{");
        }
        if l.trim_start().starts_with("return new ") && l.ends_with("ClientImpl<A>(rpc, lift);") {
            l = l.replacen("ClientImpl<A>(rpc, lift);", "ClientImpl(rpc);", 1);
        }
        out.push(l);
    }
    out.join("\n")
}

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
