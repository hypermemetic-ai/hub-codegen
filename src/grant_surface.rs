//! What each method needs (AUTH-9).
//!
//! AUTH-3 puts on every method's schema its **grant** (`<activation>.<method>`,
//! e.g. `tickets.set_status`), its **target** (the param that locates what a
//! call acts on, or none) and its **long description** (the doc comment), and
//! on every activation its default **roles**. synapse lifts them onto the IR
//! (`mdGrant`, `mdTarget`, `mdLongDescription`, `irPluginRoles`); this module
//! holds the helpers both backends use to say them in a generated client:
//!
//! - TypeScript: JSDoc (`@requires \`<grant>\` on the scope of \`<target>\``),
//!   a per-activation `requires` map, `defaultRoles`, and a typed `Forbidden`
//!   error the RPC helpers throw for an `error_kind: "forbidden"` item.
//! - Rust: doc comments with the same sentence, and a per-module `REQUIRES`
//!   table with a `requires_of` lookup.
//!
//! Surfacing only: the gate (AUTH-2) decides, server-side.
//!
//! # Absence contract
//!
//! An IR none of whose methods carries a grant, a target or a long
//! description, and with no plugin roles, generates byte-identical output to
//! codegen before AUTH-9.

use crate::ir::{MethodDef, IR};

/// True when this method says anything AUTH-9 surfaces.
pub fn method_has_surface(method: &MethodDef) -> bool {
    method.md_grant.is_some() || method.md_target.is_some() || long_description(method).is_some()
}

/// True when any method in the IR carries a grant — what turns on the typed
/// `Forbidden` (TypeScript) and the `Requirement` type (Rust).
pub fn ir_has_grants(ir: &IR) -> bool {
    ir.ir_methods.values().any(|m| m.md_grant.is_some())
}

/// The long description worth saying: present, non-blank, and not merely
/// the short description again.
pub fn long_description(method: &MethodDef) -> Option<&str> {
    let long = method.md_long_description.as_deref()?.trim();
    if long.is_empty() {
        return None;
    }
    let short = method.md_description.as_deref().unwrap_or("").trim();
    if long == short {
        return None;
    }
    Some(long)
}

/// The requirement as a sentence fragment, in backticks:
/// `` `tickets.set_status` on the scope of `id` `` with a target,
/// `` `tickets.list` anywhere it is held `` without one.
pub fn requirement_phrase(method: &MethodDef) -> Option<String> {
    let grant = method.md_grant.as_deref()?;
    Some(match method.md_target.as_deref() {
        Some(target) => format!("`{}` on the scope of `{}`", grant, target),
        None => format!("`{}` anywhere it is held", grant),
    })
}

/// A JSON string literal (double-quoted, escaped) — valid in TypeScript too.
fn lit(s: &str) -> String {
    serde_json::to_string(s).expect("a string serializes")
}

// ─────────────────────────────────────────────────────────────
// TypeScript
// ─────────────────────────────────────────────────────────────

/// The long description as JSDoc body lines (no ` * ` leader), with `*/`
/// defused so a doc comment cannot end the JSDoc block early.
pub fn ts_long_description_lines(method: &MethodDef) -> Vec<String> {
    match long_description(method) {
        Some(long) => long.lines().map(|l| l.trim_end().replace("*/", "*\\/")).collect(),
        None => Vec::new(),
    }
}

/// The `@requires` JSDoc tag for one method, when it has a grant.
pub fn ts_jsdoc(method: &MethodDef) -> Vec<String> {
    requirement_phrase(method).map(|p| vec![format!("@requires {}", p)]).unwrap_or_default()
}

/// Write one method's JSDoc block into `lines` at two-space indent:
/// the short description, then (after a blank line) the long description,
/// then the tags. Without a long description this is exactly the pre-AUTH-9
/// layout — a one-line `/** desc */` when there are no tags.
pub fn push_ts_jsdoc(lines: &mut Vec<String>, desc: Option<&str>, long: &[String], tags: &[String]) {
    match (desc, long.is_empty(), tags.is_empty()) {
        (None, true, true) => {}
        (Some(d), true, true) => lines.push(format!("  /** {} */", d)),
        _ => {
            lines.push("  /**".to_string());
            if let Some(d) = desc {
                lines.push(format!("   * {}", d));
                if !long.is_empty() {
                    lines.push("   *".to_string());
                }
            }
            for l in long {
                if l.is_empty() {
                    lines.push("   *".to_string());
                } else {
                    lines.push(format!("   * {}", l));
                }
            }
            for t in tags {
                lines.push(format!("   * {}", t));
            }
            lines.push("   */".to_string());
        }
    }
}

/// The per-activation `requires` map, keyed by grant, for the methods that
/// carry one (sorted by grant); `None` when none does.
///
/// ```ts
/// export const requires = {
///   "tickets.set_status": { grant: "tickets.set_status", target: "id" },
/// } as const;
/// ```
pub fn ts_requires_map(namespace: &str, methods: &[&MethodDef]) -> Option<String> {
    let mut entries: Vec<(&str, Option<&str>)> = methods
        .iter()
        .filter_map(|m| m.md_grant.as_deref().map(|g| (g, m.md_target.as_deref())))
        .collect();
    if entries.is_empty() {
        return None;
    }
    entries.sort();
    entries.dedup();
    let mut out = vec![
        format!(
            "/** What each {} method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */",
            namespace
        ),
        "export const requires = {".to_string(),
    ];
    for (grant, target) in entries {
        let target = target.map(lit).unwrap_or_else(|| "null".to_string());
        out.push(format!("  {}: {{ grant: {}, target: {} }},", lit(grant), lit(grant), target));
    }
    out.push("} as const;".to_string());
    Some(out.join("\n"))
}

/// The activation's default roles, when the IR carries them for `namespace`.
///
/// ```ts
/// export const defaultRoles = {
///   "editor": ["tickets.get", "tickets.set_status"],
/// } as const;
/// ```
pub fn ts_default_roles(namespace: &str, ir: &IR) -> Option<String> {
    let roles = ir.ir_plugin_roles.get(namespace).filter(|r| !r.is_empty())?;
    let mut out = vec![
        format!(
            "/** The {} activation's default roles, each the grants it lists (AUTH-3). Defaults only: a server's roles are data an admin may change, and `admin` holds every grant. */",
            namespace
        ),
        "export const defaultRoles = {".to_string(),
    ];
    for (role, grants) in roles {
        let grants = grants.iter().map(|g| lit(g)).collect::<Vec<_>>().join(", ");
        out.push(format!("  {}: [{}],", lit(role), grants));
    }
    out.push("} as const;".to_string());
    Some(out.join("\n"))
}

/// The `errorFromStreamItem` of pre-AUTH-9 codegen, verbatim — replaced by
/// [`TS_ERROR_FROM_STREAM_ITEM_WITH_FORBIDDEN`] when the IR carries grants.
pub const TS_ERROR_FROM_STREAM_ITEM: &str = r#"export function errorFromStreamItem(item: {
  message: string;
  code?: string;
  recoverable: boolean;
  metadata?: StreamMetadata;
}): PlexusError {
  switch (item.code) {"#;

/// `errorFromStreamItem` reading AUTH-2's typed refusal first, plus the
/// `Forbidden` class it constructs.
pub const TS_ERROR_FROM_STREAM_ITEM_WITH_FORBIDDEN: &str = r#"export function errorFromStreamItem(item: {
  message: string;
  code?: string;
  recoverable: boolean;
  metadata?: StreamMetadata;
  error_kind?: string;
  grant?: string;
  scope?: string;
  roles?: string[];
  held_on?: string[];
}): PlexusError {
  // AUTH-9: a refusal that names the grant it needed is a typed Forbidden.
  if (item.error_kind === 'forbidden' && typeof item.grant === 'string') {
    return new Forbidden(item.message, {
      grant: item.grant,
      scope: item.scope ?? '',
      roles: item.roles ?? [],
      heldOn: item.held_on ?? [],
    }, item.recoverable, item.metadata);
  }
  switch (item.code) {"#;

/// The optional refusal fields AUTH-2 adds to an error item, declared on
/// `PlexusStreamItemError` when the IR carries grants.
pub const TS_STREAM_ITEM_ERROR_RECOVERABLE: &str = r#"  /** Whether the operation can be retried */
  recoverable: boolean;
}"#;

/// [`TS_STREAM_ITEM_ERROR_RECOVERABLE`] with AUTH-2's refusal fields.
pub const TS_STREAM_ITEM_ERROR_RECOVERABLE_WITH_REFUSAL: &str = r#"  /** Whether the operation can be retried */
  recoverable: boolean;
  /** AUTH-2: `forbidden` or `unauthenticated` for an auth refusal */
  error_kind?: string;
  /** AUTH-2: the grant a refused call needed, e.g. `tickets.set_status` */
  grant?: string;
  /** AUTH-2: the scope it was needed on */
  scope?: string;
  /** AUTH-2: the roles that list the grant */
  roles?: string[];
  /** AUTH-2: where the caller holds the grant */
  held_on?: string[];
}"#;

/// The `Forbidden` class, appended to types.ts when the IR carries grants.
pub const TS_FORBIDDEN_CLASS: &str = r#"
// === AUTH-9: the typed refusal ===
//
// A gated service (AUTH-2) refuses a call with an error item that names what it
// needed — `{"type":"error","error_kind":"forbidden","grant":"tickets.set_status",
// "scope":"IDENTITY","roles":["editor","admin"],"held_on":["MENGER"],
// "code":"-32003","message":"needs tickets.set_status on IDENTITY",…}` — and
// the RPC helpers throw it as a `Forbidden`.

/** What a refusal says it needed (AUTH-2). */
export interface Refusal {
  /** The grant the call needed, e.g. `tickets.set_status`. */
  grant: string;
  /** The scope it was needed on, e.g. `IDENTITY`. */
  scope: string;
  /** The roles that list the grant. */
  roles: string[];
  /** Where the caller holds the grant (none: nowhere). */
  heldOn: string[];
}

/**
 * A refusal that names what it needed: the grant, the scope it was needed
 * on, the roles that list it, and where the caller holds it (AUTH-2).
 *
 * A `ForbiddenError` (and so a `PlexusError`): a handler written for those
 * still catches it.
 */
export class Forbidden extends ForbiddenError implements Refusal {
  readonly grant: string;
  readonly scope: string;
  readonly roles: string[];
  readonly heldOn: string[];
  constructor(
    message: string,
    refusal: Refusal,
    recoverable: boolean = false,
    metadata?: StreamMetadata
  ) {
    super(message, recoverable, metadata);
    this.name = 'Forbidden';
    this.grant = refusal.grant;
    this.scope = refusal.scope;
    this.roles = refusal.roles;
    this.heldOn = refusal.heldOn;
  }
}
"#;

/// Core transport types with AUTH-9's refusal when `with_grants`; exactly
/// `base` otherwise.
pub fn ts_core_types(base: &str, with_grants: bool) -> String {
    if !with_grants {
        return base.to_string();
    }
    debug_assert!(base.contains(TS_ERROR_FROM_STREAM_ITEM));
    debug_assert!(base.contains(TS_STREAM_ITEM_ERROR_RECOVERABLE));
    let mut out = base
        .replacen(TS_STREAM_ITEM_ERROR_RECOVERABLE, TS_STREAM_ITEM_ERROR_RECOVERABLE_WITH_REFUSAL, 1)
        .replacen(TS_ERROR_FROM_STREAM_ITEM, TS_ERROR_FROM_STREAM_ITEM_WITH_FORBIDDEN, 1);
    out.push_str(TS_FORBIDDEN_CLASS);
    out
}

// ─────────────────────────────────────────────────────────────
// Rust
// ─────────────────────────────────────────────────────────────

/// Doc-comment lines (without the `/// ` leader; `""` for a blank `///`)
/// for one method: the long description, then the requirement — each
/// preceded by a blank line. Empty when the method carries neither.
pub fn rust_doc(method: &MethodDef) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(long) = long_description(method) {
        lines.push(String::new());
        lines.extend(long.lines().map(|l| l.trim().to_string()));
    }
    if let Some(phrase) = requirement_phrase(method) {
        lines.push(String::new());
        lines.push(format!("Requires {}.", phrase));
    }
    lines
}

/// The `Requirement` type, appended to the generated crate's
/// `src/client.rs` when the IR carries grants.
pub fn rust_requirement_decls() -> &'static str {
    r#"// === Method requirements (AUTH-9) ===
// Surfacing only — the gate decides, server-side.

/// What a method needs (AUTH-9): its grant, and the param that locates what
/// a call acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requirement {
    /// The method's name within its activation, e.g. `"set_status"`.
    pub method: &'static str,
    /// The grant that allows it: `<activation>.<method>`, e.g. `"tickets.set_status"`.
    pub grant: &'static str,
    /// The param whose value locates what the call acts on; the grant must
    /// be held on that thing's scope. `None`: allowed wherever it is held.
    pub target: Option<&'static str>,
}"#
}

/// A module's `REQUIRES` table and `requires_of` lookup over the methods
/// that carry a grant (sorted by method name); `None` when none does.
/// Named `requires_of` so it never collides with a method named `requires`
/// (the auth activation has one).
pub fn rust_requires_table(namespace: &str, methods: &[&MethodDef]) -> Option<String> {
    let mut entries: Vec<(&str, &str, Option<&str>)> = methods
        .iter()
        .filter_map(|m| m.md_grant.as_deref().map(|g| (m.md_name.as_str(), g, m.md_target.as_deref())))
        .collect();
    if entries.is_empty() {
        return None;
    }
    entries.sort();
    let mut out = vec![
        "// === Requirements (AUTH-9) ===".to_string(),
        "".to_string(),
        format!(
            "/// What each `{}` method needs (AUTH-9): its grant, and the param that",
            namespace
        ),
        "/// locates what it acts on. Surfacing only — the gate decides, server-side.".to_string(),
        "pub const REQUIRES: &[crate::client::Requirement] = &[".to_string(),
    ];
    for (method, grant, target) in entries {
        let target = target.map(|t| format!("Some({})", lit(t))).unwrap_or_else(|| "None".to_string());
        out.push(format!(
            "    crate::client::Requirement {{ method: {}, grant: {}, target: {} }},",
            lit(method),
            lit(grant),
            target
        ));
    }
    out.push("];".to_string());
    out.push("".to_string());
    out.push(format!(
        "/// The requirement of one `{}` method, by its name or its grant.",
        namespace
    ));
    out.push("pub fn requires_of(method: &str) -> Option<&'static crate::client::Requirement> {".to_string());
    out.push("    REQUIRES.iter().find(|r| r.method == method || r.grant == method)".to_string());
    out.push("}".to_string());
    Some(out.join("\n"))
}

/// A module's `DEFAULT_ROLES` table, when the IR carries roles for it.
pub fn rust_default_roles(namespace: &str, ir: &IR) -> Option<String> {
    let roles = ir.ir_plugin_roles.get(namespace).filter(|r| !r.is_empty())?;
    let mut out = vec![
        format!(
            "/// The `{}` activation's default roles, each the grants it lists (AUTH-3).",
            namespace
        ),
        "/// Defaults only: a server's roles are data an admin may change, and".to_string(),
        "/// `admin` holds every grant.".to_string(),
        "pub const DEFAULT_ROLES: &[(&str, &[&str])] = &[".to_string(),
    ];
    for (role, grants) in roles {
        let grants = grants.iter().map(|g| lit(g)).collect::<Vec<_>>().join(", ");
        out.push(format!("    ({}, &[{}]),", lit(role), grants));
    }
    out.push("];".to_string());
    Some(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{MethodRole, TypeRef};

    fn method(name: &str, grant: Option<&str>, target: Option<&str>, long: Option<&str>) -> MethodDef {
        MethodDef {
            md_name: name.to_string(),
            md_full_path: format!("tickets.{}", name),
            md_namespace: "tickets".to_string(),
            md_description: Some("Change a ticket's status".to_string()),
            md_streaming: true,
            md_params: vec![],
            md_returns: TypeRef::RefAny,
            md_bidir_type: None,
            md_role: MethodRole::Rpc,
            md_deprecation: None,
            md_requires_credential: None,
            md_auth_posture: None,
            md_public: false,
            md_grant: grant.map(str::to_string),
            md_target: target.map(str::to_string),
            md_long_description: long.map(str::to_string),
        }
    }

    #[test]
    fn nothing_without_the_fields() {
        let m = method("get", None, None, None);
        assert!(!method_has_surface(&m));
        assert!(ts_jsdoc(&m).is_empty());
        assert!(ts_long_description_lines(&m).is_empty());
        assert!(rust_doc(&m).is_empty());
        assert!(ts_requires_map("tickets", &[&m]).is_none());
        assert!(rust_requires_table("tickets", &[&m]).is_none());
        assert_eq!(ts_core_types("base", false), "base");
    }

    #[test]
    fn the_phrase_with_and_without_a_target() {
        let with = method("set_status", Some("tickets.set_status"), Some("id"), None);
        let without = method("list", Some("tickets.list"), None, None);
        assert_eq!(ts_jsdoc(&with), vec!["@requires `tickets.set_status` on the scope of `id`"]);
        assert_eq!(ts_jsdoc(&without), vec!["@requires `tickets.list` anywhere it is held"]);
        assert_eq!(rust_doc(&with), vec!["", "Requires `tickets.set_status` on the scope of `id`."]);
    }

    #[test]
    fn a_long_description_that_repeats_the_short_one_is_dropped() {
        let m = method("set_status", None, None, Some("Change a ticket's status"));
        assert!(long_description(&m).is_none());
        let m = method("set_status", None, None, Some("Moves it */ along."));
        assert_eq!(ts_long_description_lines(&m), vec!["Moves it *\\/ along."]);
    }

    #[test]
    fn the_jsdoc_block() {
        let mut lines = Vec::new();
        push_ts_jsdoc(
            &mut lines,
            Some("Change a ticket's status"),
            &["Moves it through Pending → Ready.".to_string()],
            &["@requires `tickets.set_status` on the scope of `id`".to_string()],
        );
        assert_eq!(
            lines,
            vec![
                "  /**",
                "   * Change a ticket's status",
                "   *",
                "   * Moves it through Pending → Ready.",
                "   * @requires `tickets.set_status` on the scope of `id`",
                "   */",
            ]
        );
        let mut one = Vec::new();
        push_ts_jsdoc(&mut one, Some("desc"), &[], &[]);
        assert_eq!(one, vec!["  /** desc */"]);
    }

    #[test]
    fn the_requires_map_is_keyed_by_grant() {
        let a = method("set_status", Some("tickets.set_status"), Some("id"), None);
        let b = method("list", Some("tickets.list"), None, None);
        let map = ts_requires_map("tickets", &[&a, &b]).unwrap();
        assert!(map.contains("  \"tickets.list\": { grant: \"tickets.list\", target: null },\n  \"tickets.set_status\": { grant: \"tickets.set_status\", target: \"id\" },"));
        assert!(map.ends_with("} as const;"));
        let table = rust_requires_table("tickets", &[&a, &b]).unwrap();
        assert!(table.contains("crate::client::Requirement { method: \"set_status\", grant: \"tickets.set_status\", target: Some(\"id\") },"));
        assert!(table.contains("pub fn requires_of(method: &str)"));
    }
}
