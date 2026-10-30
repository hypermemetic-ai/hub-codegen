//! TICKETS-37: a type the service declares, used by a generated namespace.
//!
//! An IR 2.0 from synapse ≤ 3.16 keeps one copy of a type that several
//! schemas declare (one name, one structure), and when the protocol root's
//! schema declares it too, the copy it keeps is the root's: namespace `""`.
//! Since SYNAPSE-CC-9 every activation's `schema` method returns
//! `SchemaResult`, so the root declares `Scope` and `MethodPath`, and an
//! activation's own reference to its `Scope` reads `.Scope`. The root's
//! types are the transport's, and no generator writes them, so such a
//! reference was left undefined (menger's frame SDK: TS2304 on `Scope` and
//! `MethodPath`).
//!
//! [`rehome_service_types`] gives each such type a home among the generated
//! namespaces that use it, by IR 2.1's rule (SYNAPSE-CC-8): the namespace
//! whose methods and types refer to it most, a tie to the shortest name and
//! then the alphabetically first (2.1's "an ancestor of the tied" is always
//! the shortest of them, so it is the same choice), never the root. Every
//! reference is rewritten to the home, so the generators' cross-namespace
//! imports reach it, and the types it refers to follow it in later rounds.
//! A namespace that declares a type of the same name is no home for it. An
//! IR whose generated namespaces name no root type (every IR 2.1, whose
//! synapse never keeps the root's copy of one) is returned as it is.

use crate::ir::{QualifiedName, TypeKind, TypeRef, IR};
use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::BTreeMap;

/// The IR with every service-level type a generated namespace uses moved to
/// its home (see the module's doc); borrowed unchanged when there is none.
pub fn rehome_service_types(ir: &IR) -> Cow<'_, IR> {
    let mut moved: Option<IR> = None;
    loop {
        let homes = homes(moved.as_ref().unwrap_or(ir));
        if homes.is_empty() {
            break;
        }
        move_home(moved.get_or_insert_with(|| ir.clone()), &homes);
    }
    match moved {
        Some(ir) => Cow::Owned(ir),
        None => Cow::Borrowed(ir),
    }
}

/// This round's homes: each root type a generated namespace refers to, by
/// its local name, and the namespace it moves to.
fn homes(ir: &IR) -> BTreeMap<String, String> {
    // a root type's local name → namespace → references from it
    let mut refs: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for m in ir.ir_methods.values().filter(|m| !m.md_namespace.is_empty()) {
        count(ir, &mut refs, &m.md_namespace, &m.md_returns);
        // what a generator writes: the parameters a client sends (SYNAPSE-CC-2)
        for p in m.sent_params() {
            count(ir, &mut refs, &m.md_namespace, &p.pd_type);
        }
        if let Some(b) = &m.md_bidir_type {
            count(ir, &mut refs, &m.md_namespace, b);
        }
    }
    for td in ir.ir_types.values().filter(|td| !td.td_namespace.is_empty()) {
        for tr in kind_refs(&td.td_kind) {
            count(ir, &mut refs, &td.td_namespace, tr);
        }
    }
    refs.into_iter()
        .filter_map(|(name, by_ns)| {
            by_ns
                .into_iter()
                .filter(|(ns, _)| !ir.ir_types.contains_key(&format!("{ns}.{name}")))
                .min_by_key(|(ns, n)| (Reverse(*n), ns.len(), ns.clone()))
                .map(|(ns, _)| (name, ns))
        })
        .collect()
}

/// Counts each reference `tr` makes from `ns` to a type the root declares.
fn count(ir: &IR, refs: &mut BTreeMap<String, BTreeMap<String, usize>>, ns: &str, tr: &TypeRef) {
    match tr {
        TypeRef::RefNamed(qn) if qn.qn_namespace.is_empty() && ir.ir_types.contains_key(&format!(".{}", qn.qn_local_name)) => {
            *refs.entry(qn.qn_local_name.clone()).or_default().entry(ns.to_string()).or_default() += 1;
        }
        TypeRef::RefArray(inner) | TypeRef::RefOptional(inner) => count(ir, refs, ns, inner),
        _ => {}
    }
}

/// Moves each type to its home and points every reference to it there.
fn move_home(ir: &mut IR, homes: &BTreeMap<String, String>) {
    for (name, home) in homes {
        if let Some(mut td) = ir.ir_types.remove(&format!(".{name}")) {
            td.td_namespace = home.clone();
            ir.ir_types.insert(format!("{home}.{name}"), td);
        }
    }
    let rewrite = |tr: &mut TypeRef| rename(tr, homes);
    for m in ir.ir_methods.values_mut() {
        rewrite(&mut m.md_returns);
        for p in &mut m.md_params {
            rewrite(&mut p.pd_type);
        }
        if let Some(b) = &mut m.md_bidir_type {
            rewrite(b);
        }
    }
    for td in ir.ir_types.values_mut() {
        for tr in kind_refs_mut(&mut td.td_kind) {
            rewrite(tr);
        }
    }
}

fn rename(tr: &mut TypeRef, homes: &BTreeMap<String, String>) {
    match tr {
        TypeRef::RefNamed(QualifiedName { qn_namespace, qn_local_name }) if qn_namespace.is_empty() => {
            if let Some(home) = homes.get(qn_local_name.as_str()) {
                *qn_namespace = home.clone();
            }
        }
        TypeRef::RefArray(inner) | TypeRef::RefOptional(inner) => rename(inner, homes),
        _ => {}
    }
}

/// The references a type's definition makes: its fields, its variants'
/// fields, its alias's target.
fn kind_refs(kind: &TypeKind) -> Vec<&TypeRef> {
    match kind {
        TypeKind::KindStruct { ks_fields } => ks_fields.iter().map(|f| &f.fd_type).collect(),
        TypeKind::KindEnum { ke_variants, .. } => ke_variants.iter().flat_map(|v| v.vd_fields.iter().map(|f| &f.fd_type)).collect(),
        TypeKind::KindAlias { ka_target } => vec![ka_target],
        TypeKind::KindPrimitive { .. } | TypeKind::KindStringEnum { .. } => Vec::new(),
    }
}

fn kind_refs_mut(kind: &mut TypeKind) -> Vec<&mut TypeRef> {
    match kind {
        TypeKind::KindStruct { ks_fields } => ks_fields.iter_mut().map(|f| &mut f.fd_type).collect(),
        TypeKind::KindEnum { ke_variants, .. } => ke_variants.iter_mut().flat_map(|v| v.vd_fields.iter_mut().map(|f| &mut f.fd_type)).collect(),
        TypeKind::KindAlias { ka_target } => vec![ka_target],
        TypeKind::KindPrimitive { .. } | TypeKind::KindStringEnum { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{FieldDef, MethodDef, TypeDef};
    use std::collections::HashMap;

    fn named(ns: &str, name: &str) -> TypeRef {
        TypeRef::RefNamed(QualifiedName { qn_namespace: ns.into(), qn_local_name: name.into() })
    }

    fn string_alias(ns: &str, name: &str) -> (String, TypeDef) {
        let td = TypeDef {
            td_name: name.into(),
            td_namespace: ns.into(),
            td_description: None,
            td_kind: TypeKind::KindPrimitive { kp_type: "string".into(), kp_format: None },
            td_deprecation: None,
        };
        (format!("{ns}.{name}"), td)
    }

    fn holder(ns: &str, name: &str, fields: &[(&str, TypeRef)]) -> (String, TypeDef) {
        let ks_fields = fields
            .iter()
            .map(|(n, t)| FieldDef {
                fd_name: (*n).into(),
                fd_type: t.clone(),
                fd_description: None,
                fd_required: true,
                fd_default: None,
                fd_deprecation: None,
            })
            .collect();
        let td = TypeDef {
            td_name: name.into(),
            td_namespace: ns.into(),
            td_description: None,
            td_kind: TypeKind::KindStruct { ks_fields },
            td_deprecation: None,
        };
        (format!("{ns}.{name}"), td)
    }

    fn ir(types: Vec<(String, TypeDef)>) -> IR {
        IR {
            ir_version: "2.0".into(),
            ir_backend: "svc".into(),
            ir_hash: None,
            ir_metadata: None,
            ir_types: types.into_iter().collect(),
            ir_methods: HashMap::<String, MethodDef>::new(),
            ir_plugins: HashMap::new(),
            ir_plugin_deprecations: HashMap::new(),
            ir_plugin_requests: HashMap::new(),
            ir_plugin_roles: HashMap::new(),
        }
    }

    #[test]
    fn an_ir_that_names_no_root_type_is_borrowed() {
        let ir = ir(vec![string_alias("", "Scope"), holder("orgs", "Grant", &[("scope", named("orgs", "Principal"))]), string_alias("orgs", "Principal")]);
        assert!(matches!(rehome_service_types(&ir), Cow::Borrowed(_)));
    }

    #[test]
    fn the_most_references_win_then_the_shortest_name_then_the_first() {
        let ir = ir(vec![
            string_alias("", "Scope"),
            string_alias("", "Origin"),
            holder("members", "A", &[("s", named("", "Scope")), ("t", TypeRef::RefArray(Box::new(named("", "Scope"))))]),
            holder("orgs", "B", &[("s", TypeRef::RefOptional(Box::new(named("", "Scope")))), ("o", named("", "Origin"))]),
            holder("auth", "C", &[("o", named("", "Origin"))]),
        ]);
        let out = rehome_service_types(&ir);
        assert_eq!(out.ir_types["members.Scope"].td_namespace, "members");
        assert_eq!(out.ir_types["auth.Origin"].td_namespace, "auth");
        assert!(!out.ir_types.contains_key(".Scope") && !out.ir_types.contains_key(".Origin"));
        let TypeKind::KindStruct { ks_fields } = &out.ir_types["orgs.B"].td_kind else { panic!() };
        assert_eq!(ks_fields[0].fd_type, TypeRef::RefOptional(Box::new(named("members", "Scope"))));
        assert_eq!(ks_fields[1].fd_type, named("auth", "Origin"));
    }

    #[test]
    fn a_namespace_with_its_own_type_of_that_name_is_no_home() {
        let ir = ir(vec![
            string_alias("", "Scope"),
            holder("members", "Scope", &[("s", named("", "Scope")), ("t", named("", "Scope"))]),
            holder("orgs", "B", &[("s", named("", "Scope"))]),
        ]);
        let out = rehome_service_types(&ir);
        assert!(out.ir_types.contains_key("orgs.Scope"), "{:?}", out.ir_types.keys().collect::<Vec<_>>());
        assert!(matches!(out.ir_types["members.Scope"].td_kind, TypeKind::KindStruct { .. }), "members keeps its own");
    }
}
