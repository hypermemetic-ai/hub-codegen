//! Module for tickets.inputs namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

use crate::tickets::{SpikeEvent, Status, TicketId};

// === Methods ===

/// Answer an input.
///
/// Requires `inputs.answer` anywhere it is held.
pub async fn answer(client: &PlexusClient, ticket: TicketId, value: Status) -> Result<Pin<Box<dyn Stream<Item = Result<SpikeEvent>> + Send>>> {
    let stream = client.call_stream("tickets.inputs.answer", json!({ "ticket": ticket, "value": value })).await?;

    // Filter and transform stream items to typed data
    let typed_stream = stream.filter_map(|item| async move {
        match item {
            Ok(PlexusStreamItem::Data { content, .. }) => {
                match serde_json::from_value::<SpikeEvent>(content) {
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

/// A ticket's inputs.
///
/// Requires `inputs.list` anywhere it is held.
pub async fn list(client: &PlexusClient, ticket: TicketId) -> Result<Pin<Box<dyn Stream<Item = Result<SpikeEvent>> + Send>>> {
    let stream = client.call_stream("tickets.inputs.list", json!({ "ticket": ticket })).await?;

    // Filter and transform stream items to typed data
    let typed_stream = stream.filter_map(|item| async move {
        match item {
            Ok(PlexusStreamItem::Data { content, .. }) => {
                match serde_json::from_value::<SpikeEvent>(content) {
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

/// Get plugin or method schema. Pass {"method": "name"} for a specific method.
///
/// Requires `inputs.schema` anywhere it is held.
pub async fn schema(client: &PlexusClient) -> Result<serde_json::Value> {
    client.call_single("tickets.inputs.schema", serde_json::Value::Null).await
}

// === Requirements (AUTH-9) ===

/// What each `tickets.inputs` method needs (AUTH-9): its grant, and the param that
/// locates what it acts on. Surfacing only — the gate decides, server-side.
pub const REQUIRES: &[crate::client::Requirement] = &[
    crate::client::Requirement { method: "answer", grant: "inputs.answer", target: None },
    crate::client::Requirement { method: "list", grant: "inputs.list", target: None },
    crate::client::Requirement { method: "schema", grant: "inputs.schema", target: None },
];

/// The requirement of one `tickets.inputs` method, by its name or its grant.
pub fn requires_of(method: &str) -> Option<&'static crate::client::Requirement> {
    REQUIRES.iter().find(|r| r.method == method || r.grant == method)
}
