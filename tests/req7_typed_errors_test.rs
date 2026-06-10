//! REQ-7 acceptance test: generated transport emits typed error classes
//! for the four semantic JSON-RPC error codes.

use hub_codegen::generator::{GenerationOptions, GenerateSelector, TransportEnv};
use hub_codegen::ir::*;
use hub_codegen::generate_typescript;
use std::collections::HashMap;

fn opts() -> GenerationOptions {
    GenerationOptions {
        transport: TransportEnv::Ws,
        generate: GenerateSelector::All,
        plugins_filter: None,
        smoke_transport_path: "../transport".to_string(),
        backend_url: "ws://localhost:4444".to_string(),
        deprecation: Default::default(),
    }
}

fn tiny_ir() -> IR {
    let mut methods = HashMap::new();
    methods.insert("svc.ping".to_string(), MethodDef {
        md_name: "ping".to_string(),
        md_full_path: "svc.ping".to_string(),
        md_namespace: "svc".to_string(),
        md_description: Some("ping".to_string()),
        md_streaming: false,
        md_params: vec![],
        md_returns: TypeRef::RefPrimitive("string".to_string(), None),
        md_bidir_type: None,
        md_role: Default::default(),
        md_deprecation: None,
            md_requires_credential: None,
            md_auth_posture: None,
            md_public: false,
    });
    let mut plugins = HashMap::new();
    plugins.insert("svc".to_string(), vec!["ping".to_string()]);
    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("x".to_string()),
        ir_metadata: None,
        ir_types: HashMap::new(),
        ir_methods: methods,
        ir_plugins: plugins,
        ir_plugin_deprecations: HashMap::new(),
        ir_plugin_requests: HashMap::new(),
    }
}

#[test]
fn transport_emits_all_typed_error_classes() {
    let out = generate_typescript(&tiny_ir(), &opts()).unwrap();
    let transport = out.files.get("transport.ts").expect("transport.ts must exist");

    for cls in &[
        "class PlexusRpcError",
        "class AuthenticationError",
        "class InvalidParamsError",
        "class MethodNotFoundError",
        "class ExecutionError",
    ] {
        assert!(
            transport.contains(cls),
            "REQ-7: transport must declare `{}`. Missing from:\n{}",
            cls, transport
        );
    }
}

#[test]
fn transport_dispatches_typed_errors_by_code() {
    let out = generate_typescript(&tiny_ir(), &opts()).unwrap();
    let transport = out.files.get("transport.ts").unwrap();

    // A dispatcher switches on the error code and constructs the right subclass.
    assert!(
        transport.contains("function rpcErrorFor"),
        "REQ-7: transport must declare rpcErrorFor dispatcher. Got:\n{}",
        transport
    );
    for code in &["-32001", "-32602", "-32601", "-32000"] {
        assert!(
            transport.contains(&format!("case {}", code)),
            "REQ-7: dispatcher must map code {}. Got:\n{}",
            code, transport
        );
    }
}

#[test]
fn transport_rejects_errors_with_typed_error() {
    let out = generate_typescript(&tiny_ir(), &opts()).unwrap();
    let transport = out.files.get("transport.ts").unwrap();

    // On error, the pending request gets rejected with rpcErrorFor(...),
    // not with a bare Error (which was the pre-REQ-7 behavior).
    assert!(
        transport.contains("pending.reject(rpcErrorFor("),
        "REQ-7: handleResponse must reject with the typed error dispatcher. Got:\n{}",
        transport
    );
    assert!(
        !transport.contains("pending.reject(new Error(`RPC error"),
        "REQ-7: legacy string-error path must be gone. Got:\n{}",
        transport
    );
}
