//! RETURNS-16: a generated Rust tagged enum round-trips the exact wire tag
//! of every variant.
//!
//! The generator used to write `#[serde(tag = "type", rename_all =
//! "lowercase")]`, so the variant `StatusChanged` (wire tag
//! `status_changed`, menger's `TicketsEvent`) read and wrote
//! `"statuschanged"`. It now renames each variant to its tag explicitly.
//!
//! How the round trip is proven without building a whole generated crate:
//! the enum the generator emits for the fixture is pinned byte-for-byte to
//! `tests/golden/returns16/rust_tickets_event.rs`, and that same file is
//! `include!`d below — so the code compiled into this test *is* the
//! generator's output, and real wire frames go through it with serde_json.
//! (After `UPDATE_GOLDEN=1` rewrites the golden, run the tests once more:
//! `include!` read the file at compile time.)

use hub_codegen::generator::rust::types::generate_typedef;
use hub_codegen::{generate_rust, IR};
use serde_json::{json, Value};
use std::path::PathBuf;

/// The generator's output for the fixture's enum, compiled.
#[allow(dead_code)]
mod generated {
    use serde::{Deserialize, Serialize};
    include!("golden/returns16/rust_tickets_event.rs");
}
use generated::TicketsEvent;

/// A `tickets` namespace whose event enum carries menger's tag shapes: the
/// three the ticket names (`created`, `status_changed`, `lagged`), another
/// multi-word struct variant and a multi-word unit variant.
const FIXTURE: &str = r#"{
  "irVersion": "2.0",
  "irBackend": "fixture",
  "irPlugins": {"tickets": ["watch"]},
  "irMethods": {
    "tickets.watch": {
      "mdName": "watch", "mdFullPath": "tickets.watch", "mdNamespace": "tickets",
      "mdStreaming": true, "mdParams": [],
      "mdReturns": {"tag": "RefNamed", "contents": {"qnNamespace": "tickets", "qnLocalName": "TicketsEvent"}}
    }
  },
  "irTypes": {
    "tickets.TicketsEvent": {
      "tdName": "TicketsEvent", "tdNamespace": "tickets",
      "tdDescription": "Events from the tickets activation",
      "tdKind": {"tag": "KindEnum", "keDiscriminator": "type", "keVariants": [
        {"vdName": "created", "vdDescription": "A ticket was created", "vdFields": [
          {"fdName": "id", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true},
          {"fdName": "title", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true}
        ]},
        {"vdName": "status_changed", "vdDescription": "A ticket's status changed", "vdFields": [
          {"fdName": "id", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true},
          {"fdName": "previous", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true}
        ]},
        {"vdName": "commits_linked", "vdFields": [
          {"fdName": "id", "fdType": {"tag": "RefPrimitive", "contents": ["string", null]}, "fdRequired": true},
          {"fdName": "added", "fdType": {"tag": "RefArray", "contents": {"tag": "RefPrimitive", "contents": ["string", null]}}, "fdRequired": true}
        ]},
        {"vdName": "caught_up", "vdDescription": "The watch has replayed its backlog", "vdFields": []},
        {"vdName": "lagged", "vdFields": [
          {"fdName": "missed", "fdType": {"tag": "RefPrimitive", "contents": ["integer", "uint64"]}, "fdRequired": true}
        ]}
      ]}
    }
  }
}"#;

fn fixture() -> IR {
    serde_json::from_str(FIXTURE).expect("fixture IR decodes")
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("returns16")
        .join("rust_tickets_event.rs")
}

/// The generated enum is the golden that `generated` compiles — and it is
/// what lands in the generated client's `src/tickets/mod.rs`.
#[test]
fn generated_enum_matches_golden() {
    let ir = fixture();
    let typedef = generate_typedef(&ir.ir_types["tickets.TicketsEvent"]);
    // The golden file ends with a newline; the generated item does not.
    let actual = format!("{typedef}\n");

    let path = golden_path();
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("create golden dir");
        std::fs::write(&path, &actual).expect("write golden file");
    } else {
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("missing golden file {} ({e}); regenerate with UPDATE_GOLDEN=1 cargo test", path.display())
        });
        assert_eq!(
            actual,
            expected,
            "generated enum diverged from golden {} — if intentional, regenerate with UPDATE_GOLDEN=1 cargo test \
             (twice: the round-trip test include!s the golden)",
            path.display()
        );
    }

    let client = generate_rust(&ir).expect("rust generation");
    let module = &client.files["src/tickets/mod.rs"];
    assert!(module.contains(&typedef), "src/tickets/mod.rs does not carry the enum:\n{module}");
    assert!(!module.contains("rename_all"), "no container rename_all may remain:\n{module}");
    for tag in ["created", "status_changed", "commits_linked", "caught_up", "lagged"] {
        assert!(
            typedef.contains(&format!("#[serde(rename = \"{tag}\")]")),
            "variant `{tag}` not renamed to its wire tag:\n{typedef}"
        );
    }
}

/// Wire frames decode into the right variant and encode back to the same
/// JSON, tag included.
#[test]
fn every_wire_tag_round_trips() {
    let cases: Vec<(Value, fn(&TicketsEvent) -> bool)> = vec![
        (
            json!({"type": "created", "id": "RETURNS-16", "title": "snake_case tags"}),
            |e| matches!(e, TicketsEvent::Created { id, title } if id == "RETURNS-16" && title == "snake_case tags"),
        ),
        (
            json!({"type": "status_changed", "id": "RETURNS-16", "previous": "Ready"}),
            |e| matches!(e, TicketsEvent::StatusChanged { id, previous } if id == "RETURNS-16" && previous == "Ready"),
        ),
        (
            json!({"type": "commits_linked", "id": "RETURNS-16", "added": ["hub-codegen@0000000"]}),
            |e| matches!(e, TicketsEvent::CommitsLinked { added, .. } if added == &["hub-codegen@0000000"]),
        ),
        (json!({"type": "caught_up"}), |e| matches!(e, TicketsEvent::CaughtUp)),
        (
            json!({"type": "lagged", "missed": 3}),
            |e| matches!(e, TicketsEvent::Lagged { missed: 3 }),
        ),
    ];

    for (wire, is_expected) in cases {
        let event: TicketsEvent = serde_json::from_value(wire.clone())
            .unwrap_or_else(|e| panic!("{wire} does not decode: {e}"));
        assert!(is_expected(&event), "{wire} decoded as {event:?}");
        let back = serde_json::to_value(&event).expect("encode");
        assert_eq!(back, wire, "{event:?} does not encode back to its wire form");
    }
}

/// The `lowercase` spellings the generator used to accept are not tags.
#[test]
fn collapsed_tags_are_rejected() {
    for wire in [
        json!({"type": "statuschanged", "id": "RETURNS-16", "previous": "Ready"}),
        json!({"type": "commitslinked", "id": "RETURNS-16", "added": []}),
        json!({"type": "caughtup"}),
    ] {
        assert!(
            serde_json::from_value::<TicketsEvent>(wire.clone()).is_err(),
            "{wire} should not decode"
        );
    }
}
