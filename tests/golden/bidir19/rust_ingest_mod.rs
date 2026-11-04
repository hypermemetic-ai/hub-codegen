//! Module for ingest namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

// === Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IngestEvent {
    /// Every item taken, and how long each took to reach the method.
    #[serde(rename = "taken")]
    Taken {
items: u64,
p50_us: f64,
p99_us: f64,
max_us: f64,
    },
    /// Every chunk taken and its bytes.
    #[serde(rename = "received")]
    Received {
items: u64,
bytes: u64,
p50_us: f64,
p99_us: f64,
p999_us: f64,
max_us: f64,
hist: Vec<(u64, u64)>,
    },
    /// A running count of the tokens taken.
    #[serde(rename = "tokens")]
    Tokens {
count: u64,
    },
    #[serde(rename = "error")]
    Error {
message: String,
    },
}

/// One item in: when its frame was sent, and a pad of the scenario's size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub at: u64,
    pub pad: String,
}

// === Methods ===

/// BIDIR-17's raw ingress: chunks as they come, each beginning with
/// when it was sent (CLOCK_MONOTONIC ns, big-endian). The method only
/// consumes: binary frames and base64 JSON reach it alike.
///
/// Requires `ingest.bytes` anywhere it is held.
///
/// `input`: the input half (plexus-wire spec/10), sent as the call runs, only while the service's credit allows; each chunk one binary frame.
pub async fn bytes(client: &PlexusClient, input: impl Stream<Item = ::bytes::Bytes> + Send + 'static) -> Result<IngestEvent> {
    client.call_single_with_input("ingest.bytes", serde_json::Value::Null, PlexusClient::binary_input(input)).await
}

/// Take items as they come; answer how late they came.
///
/// Requires `ingest.items` anywhere it is held.
///
/// `input`: the input half (plexus-wire spec/10), sent as the call runs, only while the service's credit allows.
pub async fn items(client: &PlexusClient, input: impl Stream<Item = Item> + Send + 'static) -> Result<IngestEvent> {
    client.call_single_with_input("ingest.items", serde_json::Value::Null, PlexusClient::json_input(input)).await
}

/// Count tokens as they come, one item a running count.
///
/// Requires `ingest.tokens` anywhere it is held.
///
/// `input`: the input half (plexus-wire spec/10), sent as the call runs, only while the service's credit allows.
pub async fn tokens(client: &PlexusClient, run: String, limit: i64, meta: std::collections::BTreeMap<String, String>, input: impl Stream<Item = String> + Send + 'static) -> Result<Pin<Box<dyn Stream<Item = Result<IngestEvent>> + Send>>> {
    let stream = client.call_stream_with_input("ingest.tokens", json!({ "run": run, "limit": limit, "meta": meta }), PlexusClient::json_input(input)).await?;

    // Filter and transform stream items to typed data
    let typed_stream = stream.filter_map(|item| async move {
        match item {
            Ok(PlexusStreamItem::Data { content, .. }) => {
                match serde_json::from_value::<IngestEvent>(content) {
                    Ok(data) => Some(Ok(data)),
                    Err(e) => Some(Err(e.into())),
                }
            }
            Ok(PlexusStreamItem::Error { message, code, .. }) => {
                Some(Err(anyhow!("Plexus error{}: {}",
                    code.map(|c| format!(" [{}]", c)).unwrap_or_default(),
                    message
                )))
            }
            Ok(PlexusStreamItem::Progress { .. }) => None, // Skip progress
            Ok(PlexusStreamItem::Done { .. }) => None, // Stream will end
            Err(e) => Some(Err(e)),
        }
    });

    Ok(Box::pin(typed_stream))
}

// === Requirements (AUTH-9) ===

/// What each `ingest` method needs (AUTH-9): its grant, and the param that
/// locates what it acts on. Surfacing only — the gate decides, server-side.
pub const REQUIRES: &[crate::client::Requirement] = &[
    crate::client::Requirement { method: "bytes", grant: "ingest.bytes", target: None },
    crate::client::Requirement { method: "items", grant: "ingest.items", target: None },
    crate::client::Requirement { method: "tokens", grant: "ingest.tokens", target: None },
];

/// The requirement of one `ingest` method, by its name or its grant.
pub fn requires_of(method: &str) -> Option<&'static crate::client::Requirement> {
    REQUIRES.iter().find(|r| r.method == method || r.grant == method)
}
