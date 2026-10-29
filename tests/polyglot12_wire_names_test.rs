//! POLYGLOT-12 · the TS transport's item types name the wire's fields as the
//! wire does (plexus-wire spec/03): `content_type`, `metadata.plexus_hash` —
//! snake_case, but for the `request` item's (`requestId`, `requestData`,
//! `timeoutMs`), which plexus-core sends in camelCase. Under every
//! transport, with grants in the IR or without. Its other half — the
//! `call_unsub` the transport never sent — landed with RETURNS-38; the kit's
//! `client.unsubscribes` holds it (plexus-wire), and a request item that
//! arrives before its call's subscription id is answered, not yielded
//! (`tests/ts_returns17/runtime.test.ts`).

use service_codegen::generator::{GenerationOptions, TransportEnv};
use service_codegen::{generate_typescript, IR};
use std::path::PathBuf;

fn fixture(name: &str) -> IR {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("read fixture")).expect("the fixture decodes")
}

#[test]
fn item_types_use_wire_names() {
    // returns17_ir carries grants (AUTH-9's types.ts); returns38_watch_ir none
    for fx in ["returns17_ir", "returns38_watch_ir"] {
        for transport in [TransportEnv::Ws, TransportEnv::Browser, TransportEnv::None] {
            let files = generate_typescript(&fixture(fx), &GenerationOptions { transport, ..GenerationOptions::default() })
                .expect("generate")
                .files;
            let types = &files["types.ts"];
            assert!(types.contains("  plexus_hash: string;"), "{fx} {transport:?}");
            assert!(types.contains("  content_type: string;"), "{fx} {transport:?}");
            assert!(!types.contains("plexusHash") && !types.contains("contentType"), "{fx} {transport:?}");
            // the request item is plexus-core's camelCase, as the wire sends it
            for f in ["  requestId: string;", "  requestData: StandardRequest;", "  timeoutMs: number;"] {
                assert!(types.contains(f), "{fx} {transport:?}: {f}");
            }
        }
    }
}

#[test]
fn a_request_item_before_the_subscription_id_is_answered() {
    let files = generate_typescript(&fixture("returns17_ir"), &GenerationOptions { transport: TransportEnv::Browser, ..GenerationOptions::default() })
        .expect("generate")
        .files;
    let t = &files["transport.ts"];
    assert!(t.contains("if (msg.type === 'request') { this.handleBidirectionalRequest(msg as PlexusStreamItemRequest); continue; }"), "{t}");
}

#[test]
fn a_notification_method_is_any_string() {
    let files = generate_typescript(&fixture("returns17_ir"), &GenerationOptions { transport: TransportEnv::Browser, ..GenerationOptions::default() })
        .expect("generate")
        .files;
    let t = &files["transport.ts"];
    assert!(!t.contains("method: 'subscription';"), "spec/01: a notification's method is not significant");
    assert!(t.contains("interface JsonRpcNotification {\n  jsonrpc: '2.0';\n  /** Not significant (plexus-wire spec/01): any string; never read (POLYGLOT-12). */\n  method: string;"));
}
