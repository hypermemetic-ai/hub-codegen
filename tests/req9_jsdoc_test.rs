//! REQ-9 acceptance tests: hub-codegen TypeScript emits JSDoc breadcrumbs
//! per method from each ParamDef's `pd_source` annotation.

use hub_codegen::generator::{GenerationOptions, GenerateSelector, TransportEnv};
use hub_codegen::ir::*;
use hub_codegen::generate_typescript;
use std::collections::HashMap;

fn default_options() -> GenerationOptions {
    GenerationOptions {
        transport: TransportEnv::Ws,
        generate: GenerateSelector::All,
        plugins_filter: None,
        smoke_transport_path: "../transport".to_string(),
        backend_url: "ws://localhost:4444".to_string(),
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
        ir_plugin_requests: HashMap::new(),
    }
}

fn method(name: &str, params: Vec<ParamDef>) -> MethodDef {
    MethodDef {
        md_name: name.to_string(),
        md_full_path: format!("svc.{}", name),
        md_namespace: "svc".to_string(),
        md_description: Some(format!("The {} method", name)),
        md_streaming: false,
        md_params: params,
        md_returns: TypeRef::RefPrimitive("string".to_string(), None),
        md_bidir_type: None,
        md_role: Default::default(),
        md_deprecation: None,
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

fn sourced_param(name: &str, source_json: serde_json::Value) -> ParamDef {
    ParamDef {
        pd_name: name.to_string(),
        pd_type: TypeRef::RefPrimitive("string".to_string(), None),
        pd_description: None,
        pd_required: false,
        pd_default: None,
        pd_deprecation: None,
        pd_source: Some(source_json),
    }
}

#[test]
fn auth_sourced_param_emits_requires_auth_jsdoc() {
    // Given: a method with an auth-sourced param
    let m = method("list", vec![
        sourced_param("user", serde_json::json!({
            "from": "auth",
            "resolver": "self.db.validate_user"
        })),
        rpc_param("search"),
    ]);
    let ir = ir_with_methods(vec![m]);

    // When: TS is generated
    let out = generate_typescript(&ir, &default_options()).unwrap();
    let client = out.files.get("svc/client.ts").expect("svc/client.ts must exist");

    // Then: JSDoc contains @requiresAuth with the resolver expression
    assert!(
        client.contains("@requiresAuth"),
        "REQ-9: method with x-plexus-source.from=auth must emit @requiresAuth JSDoc. Got:\n{}",
        client
    );
    assert!(
        client.contains("self.db.validate_user"),
        "REQ-9: @requiresAuth should carry resolver expression. Got:\n{}",
        client
    );
}

#[test]
fn cookie_sourced_param_emits_reads_cookie_jsdoc() {
    let m = method("list", vec![
        sourced_param("auth_token", serde_json::json!({
            "from": "cookie",
            "key": "access_token"
        })),
    ]);
    let ir = ir_with_methods(vec![m]);
    let out = generate_typescript(&ir, &default_options()).unwrap();
    let client = out.files.get("svc/client.ts").unwrap();

    assert!(
        client.contains("@reads-cookie access_token"),
        "REQ-9: cookie-sourced param must emit @reads-cookie with the cookie key. Got:\n{}",
        client
    );
}

#[test]
fn derived_sourced_params_emit_server_derived_jsdoc() {
    let m = method("list", vec![
        sourced_param("origin", serde_json::json!({ "from": "derived" })),
        sourced_param("client_ip", serde_json::json!({ "from": "derived" })),
    ]);
    let ir = ir_with_methods(vec![m]);
    let out = generate_typescript(&ir, &default_options()).unwrap();
    let client = out.files.get("svc/client.ts").unwrap();

    assert!(
        client.contains("@server-derived origin"),
        "REQ-9: derived-sourced param must emit @server-derived. Got:\n{}",
        client
    );
    assert!(
        client.contains("@server-derived client_ip"),
        "REQ-9: derived-sourced params each get their own @server-derived line. Got:\n{}",
        client
    );
}

#[test]
fn method_with_only_rpc_params_emits_no_source_jsdoc() {
    // Fixes the health.check false-positive: a method with no
    // x-plexus-source annotations should NOT get any server-derived
    // or auth tags, even if the activation has psRequest elsewhere.
    let m = method("check", vec![rpc_param("query")]);
    let ir = ir_with_methods(vec![m]);
    let out = generate_typescript(&ir, &default_options()).unwrap();
    let client = out.files.get("svc/client.ts").unwrap();

    assert!(
        !client.contains("@requiresAuth"),
        "method with no source annotations must not emit @requiresAuth"
    );
    assert!(
        !client.contains("@server-derived"),
        "method with no source annotations must not emit @server-derived"
    );
    assert!(
        !client.contains("@reads-cookie"),
        "method with no source annotations must not emit @reads-cookie"
    );
}

#[test]
fn mixed_source_types_all_emit_their_respective_tags() {
    // Single method with all five source types + an RPC param.
    let m = method("mixed", vec![
        sourced_param("scope", serde_json::json!({ "from": "auth", "resolver": "validate" })),
        sourced_param("session", serde_json::json!({ "from": "cookie", "key": "sid" })),
        sourced_param("origin_hdr", serde_json::json!({ "from": "header", "key": "origin" })),
        sourced_param("tenant", serde_json::json!({ "from": "query", "key": "t" })),
        sourced_param("peer_ip", serde_json::json!({ "from": "derived" })),
        rpc_param("query"),
    ]);
    let ir = ir_with_methods(vec![m]);
    let out = generate_typescript(&ir, &default_options()).unwrap();
    let client = out.files.get("svc/client.ts").unwrap();

    assert!(client.contains("@requiresAuth"),    "mixed method must emit @requiresAuth");
    assert!(client.contains("@reads-cookie sid"), "mixed method must emit @reads-cookie sid");
    assert!(client.contains("@reads-header origin"), "mixed method must emit @reads-header origin");
    assert!(client.contains("@reads-query t"),   "mixed method must emit @reads-query t");
    assert!(client.contains("@server-derived peer_ip"), "mixed method must emit @server-derived peer_ip");
}

#[test]
fn per_method_sources_take_precedence_over_activation_level_fallback() {
    // When a method has per-param pd_source, the activation-level
    // ir_plugin_requests fallback is ignored.
    let m = method("list", vec![
        sourced_param("user", serde_json::json!({ "from": "auth", "resolver": "validate" })),
    ]);
    let mut ir = ir_with_methods(vec![m]);
    // Add a legacy activation-level psRequest that, if consumed, would
    // produce a different set of tags:
    ir.ir_plugin_requests.insert(
        "svc".to_string(),
        serde_json::json!({
            "properties": {
                "legacy_field": { "x-plexus-source": { "from": "derived" } }
            },
            "required": []
        }),
    );

    let out = generate_typescript(&ir, &default_options()).unwrap();
    let client = out.files.get("svc/client.ts").unwrap();

    // Per-method annotations fire:
    assert!(client.contains("@requiresAuth"),
        "per-method annotations must fire. Got:\n{}", client);
    // Activation-level fallback does NOT also fire:
    assert!(!client.contains("legacy_field"),
        "activation-level fallback must be suppressed when per-method sources exist. Got:\n{}", client);
}

#[test]
fn activation_level_fallback_fires_only_when_no_per_method_sources() {
    // Legacy backend: no per-param pd_source anywhere, but the activation
    // has psRequest. Fallback path kicks in.
    let m = method("list", vec![rpc_param("search")]);
    let mut ir = ir_with_methods(vec![m]);
    ir.ir_plugin_requests.insert(
        "svc".to_string(),
        serde_json::json!({
            "properties": {
                "origin": { "x-plexus-source": { "from": "derived" } }
            },
            "required": []
        }),
    );

    let out = generate_typescript(&ir, &default_options()).unwrap();
    let client = out.files.get("svc/client.ts").unwrap();

    assert!(
        client.contains("@server-derived origin"),
        "activation-level fallback must fire when no per-method sources exist. Got:\n{}",
        client
    );
}
