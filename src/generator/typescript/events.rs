//! RETURNS-46 · the aggregate up the tree, in TypeScript.
//!
//! Every generated namespace gets `<ns>/events.ts`, exporting its aggregate
//! `{Ns}Event` — RETURNS-1's rule, the shape plexus-macros derives in Rust:
//! a union with one member per method, `{ method: '<wire name>'; event: I }`,
//! `I` the method's item (`Result<T, E>` when the method has an error type,
//! the `{ ok } | { err }` its items are on the wire); then one per child,
//! carrying the child's own aggregate:
//!
//! ```ts
//! export type TreeEvent =
//!   | { method: 'count'; event: Tick }
//!   | { method: 'branch'; event: TreeBranchEvent }                       // a static child
//!   | { method: 'item'; event: { name: string; event: unknown } };       // a dynamic child
//! ```
//!
//! The same JSON as the Rust aggregate's, level by level
//! (`plexus-macros/fixtures/aggregate-tree/expected/`). Read from IR 3.0
//! alone: a method's role says which members are children, a static child's
//! namespace is its full path (`tree.branch`), and the child's aggregate is
//! named by the rule its client is (`TreeBranchClient` → `TreeBranchEvent`).
//!
//! A dynamic child's event is `unknown` unless the IR describes the child's
//! activation: IR 3.0 names the dynamic child (`tree.item`, role
//! `dynamic_child`, returns `RefChild`) but carries none of its methods — a
//! service lists only its static children's schemas. An IR that names the
//! child's namespace by its return type (IR-9's form) gets that namespace's
//! aggregate.
//!
//! The auto-generated `schema` method is not a member, as it is no variant
//! of the Rust aggregate. The name is `{Ns}Event` unless the namespace
//! already has a type of that name (an activation's own event enum, written
//! before 0.7 — menger's `TicketsEvent`); then it is `{Ns}Aggregate`, the
//! Rust rule.

use super::namespaces::{dynamic_child_namespace, static_child_namespace, to_pascal};
use crate::ir::{MethodDef, MethodRole, TypeRef, IR};
use std::collections::{BTreeMap, BTreeSet};

/// The method every activation's schema lists and the aggregate leaves out.
const SCHEMA_METHOD: &str = "schema";

/// The aggregate's name for `namespace`.
pub fn aggregate_name(ir: &IR, namespace: &str) -> String {
    let base = to_pascal(namespace);
    let name = format!("{base}Event");
    let taken = ir.ir_types.values().any(|td| td.td_namespace == namespace && to_pascal(&td.td_name) == name);
    if taken {
        format!("{base}Aggregate")
    } else {
        name
    }
}

/// `<ns>/events.ts` for `namespace`, whose methods are `methods`;
/// `generated` is every namespace this run writes (a child outside it is
/// `unknown`, since its `events.ts` will not exist).
pub fn generate_events(namespace: &str, methods: &[&MethodDef], ir: &IR, generated: &BTreeSet<String>) -> String {
    let to_root = "../".repeat(namespace.matches('.').count() + 1);
    let mut sorted: Vec<&MethodDef> = methods.to_vec();
    sorted.sort_by(|a, b| a.md_name.cmp(&b.md_name));

    let mut local: BTreeSet<String> = BTreeSet::new();
    let mut cross: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut children: BTreeMap<String, String> = BTreeMap::new();
    let mut uses_result = false;
    let mut uses_needs_input = false;
    let mut items: Vec<String> = Vec::new();
    let mut statics: Vec<String> = Vec::new();
    let mut dynamics: Vec<String> = Vec::new();

    for m in sorted {
        match &m.md_role {
            MethodRole::Rpc if m.md_name == SCHEMA_METHOD => {}
            MethodRole::Rpc => {
                collect(&m.md_returns, namespace, &mut local, &mut cross);
                let ok = m.md_returns.to_ts_in_namespace(namespace);
                let item = match &m.md_error {
                    Some(err) => {
                        collect(err, namespace, &mut local, &mut cross);
                        uses_result = true;
                        format!("Result<{ok}, {}>", err.to_ts_in_namespace(namespace))
                    }
                    None => ok,
                };
                // ASK-1: a method that asks may finish with a proposal.
                let item = if m.md_asks {
                    uses_needs_input = true;
                    format!("{item} | NeedsInput")
                } else {
                    item
                };
                items.push(format!("  | {{ method: '{}'; event: {item} }}", m.md_name));
            }
            MethodRole::StaticChild => {
                let event = child_event(static_child_namespace(m, ir), ir, generated, &mut children);
                statics.push(format!("  | {{ method: '{}'; event: {event} }}", m.md_name));
            }
            MethodRole::DynamicChild { .. } => {
                let event = child_event(dynamic_child_namespace(m, ir), ir, generated, &mut children);
                dynamics.push(format!("  | {{ method: '{}'; event: {{ name: string; event: {event} }} }}", m.md_name));
            }
        }
    }

    let mut out = vec![
        "// Auto-generated by service-codegen".to_string(),
        "// Do not edit manually".to_string(),
        String::new(),
    ];
    let mut imports = Vec::new();
    if uses_result {
        imports.push(format!("import type {{ Result }} from '{to_root}rpc';"));
    }
    if uses_needs_input {
        imports.push(format!("import type {{ NeedsInput }} from '{to_root}types';"));
    }
    if !local.is_empty() {
        imports.push(format!("import type {{ {} }} from './types';", local.into_iter().collect::<Vec<_>>().join(", ")));
    }
    for (ns, names) in cross {
        imports.push(format!(
            "import type {{ {} }} from '{to_root}{}/types';",
            names.into_iter().collect::<Vec<_>>().join(", "),
            ns.replace('.', "/")
        ));
    }
    for (name, ns) in children {
        imports.push(format!("import type {{ {name} }} from '{to_root}{}/events';", ns.replace('.', "/")));
    }
    if !imports.is_empty() {
        out.extend(imports);
        out.push(String::new());
    }

    let name = aggregate_name(ir, namespace);
    out.push("/**".to_string());
    out.push(format!(" * The `{namespace}` activation's event (RETURNS-1, RETURNS-46): one member per"));
    out.push(" * method, tagged with its wire name and carrying its item; one per static".to_string());
    out.push(" * child, carrying the child's aggregate; one per dynamic child, carrying the".to_string());
    out.push(" * instance's name and its aggregate. The JSON the Rust aggregate is.".to_string());
    out.push(" */".to_string());
    let members: Vec<String> = items.into_iter().chain(statics).chain(dynamics).collect();
    if members.is_empty() {
        out.push(format!("export type {name} = never;"));
    } else {
        out.push(format!("export type {name} ="));
        let last = members.len() - 1;
        for (i, m) in members.into_iter().enumerate() {
            out.push(if i == last { format!("{m};") } else { m });
        }
    }
    out.push(String::new());
    out.join("\n")
}

/// A child's aggregate, imported, or `unknown` when this run does not write it.
fn child_event(
    child_ns: Option<String>,
    ir: &IR,
    generated: &BTreeSet<String>,
    children: &mut BTreeMap<String, String>,
) -> String {
    match child_ns {
        Some(ns) if generated.contains(&ns) => {
            let name = aggregate_name(ir, &ns);
            children.insert(name.clone(), ns);
            name
        }
        _ => "unknown".to_string(),
    }
}

/// The named types `tr` mentions: this namespace's to `local`, another's to `cross`.
fn collect(tr: &TypeRef, namespace: &str, local: &mut BTreeSet<String>, cross: &mut BTreeMap<String, BTreeSet<String>>) {
    match tr {
        TypeRef::RefNamed(qn) => {
            let name = to_pascal(qn.local_name());
            match qn.namespace() {
                Some(ns) if ns != namespace => {
                    cross.entry(ns.to_string()).or_default().insert(name);
                }
                _ => {
                    local.insert(name);
                }
            }
        }
        other => other.children().into_iter().for_each(|c| collect(c, namespace, local, cross)),
    }
}
