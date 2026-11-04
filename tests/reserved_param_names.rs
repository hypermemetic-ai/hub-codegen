//! RETURNS-40 review #6: menger's `quota.set` takes a param named `default`
//! — a word TypeScript reserves, so the generated client named it in a
//! parameter list and did not parse (TS1359). A param whose name is reserved
//! is `<name>_` in TypeScript, sent under its own name on the wire.

use service_codegen::generator::{GenerationOptions, GenerateSelector, TransportEnv};
use service_codegen::ir::*;
use service_codegen::generate_typescript;
use std::collections::HashMap;

fn default_options() -> GenerationOptions {
    GenerationOptions {
        transport: TransportEnv::Ws,
        generate: GenerateSelector::All,
        plugins_filter: None,
        smoke_transport_path: "../transport".to_string(),
        backend_url: None,
        deprecation: Default::default(),
    }
}

/// Constructs an IR with a single plugin "svc" and the given methods.
fn ir_with_methods(methods: Vec<MethodDef>) -> IR {
    let mut ir_methods = HashMap::new();
    let mut ir_plugins_methods: Vec<String> = Vec::new();
    for m in methods {
        ir_plugins_methods.push(m.md_name.clone());
        ir_methods.insert(m.md_full_path.clone(), m);
    }
    let mut ir_plugins = HashMap::new();
    ir_plugins.insert("svc".to_string(), ir_plugins_methods);
    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("req9".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods,
        ir_plugins,
        ir_plugin_deprecations: HashMap::new(),
        ir_plugin_requests: HashMap::new(), ir_plugin_roles: Default::default(),
    }
}

fn method(name: &str, params: Vec<ParamDef>) -> MethodDef {
    MethodDef {
        md_name: name.to_string(),
        md_full_path: format!("svc.{}", name),
        md_namespace: "svc".to_string(),
        md_description: Some(format!("The {} method", name)),
        md_cardinality: Cardinality::One, md_error: None, md_progress: None,
        md_params: params,
        md_returns: TypeRef::RefPrimitive("string".to_string(), None),
        md_bidir_type: None,
        md_role: Default::default(),
        md_deprecation: None,
            md_requires_credential: None,
            md_auth_posture: None,
            md_public: false, md_grant: None, md_target: None, md_long_description: None,
    }
}

fn rpc_param(name: &str) -> ParamDef {
    ParamDef {
        pd_name: name.to_string(),
        pd_type: TypeRef::RefPrimitive("string".to_string(), None),
        pd_description: None,
        pd_required: true,
        pd_default: None,
        pd_deprecation: None,
        pd_source: None,
    }
}

fn optional(name: &str) -> ParamDef {
    ParamDef { pd_required: false, ..rpc_param(name) }
}

#[test]
fn a_reserved_param_name_is_a_typescript_identifier_and_its_own_name_on_the_wire() {
    let m = method("set", vec![rpc_param("class"), optional("default"), optional("in_scope")]);
    let ir = ir_with_methods(vec![m]);
    let out = generate_typescript(&ir, &default_options()).unwrap();
    let client = out.files.get("svc/client.ts").expect("svc/client.ts must exist");
    assert!(client.contains("set(class_: string, default_?: string, inScope?: string)"), "the signature:\n{client}");
    assert!(client.contains("{ class: class_, default: default_, in_scope: inScope }"), "the wire params:\n{client}");
    for bad in ["(class:", " default?:", "(default?:"] {
        assert!(!client.contains(bad), "a reserved word as an identifier ({bad}):\n{client}");
    }
}
