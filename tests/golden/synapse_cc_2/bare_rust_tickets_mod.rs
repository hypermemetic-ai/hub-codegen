//! Module for tickets namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

// Child namespaces
pub mod inputs;

// === Types ===

/// A ticket's status — canonical spellings; `active` and `in review` are
/// read too.
/// 
/// The schema: by default the derived closed `enum` of the canonical
/// spellings; `--features schema-anyof` / `schema-examples` swap in the two
/// open shapes question 5 compares (`run.sh` runs all three).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Status {
    #[serde(rename = "Pending")]
    Pending,
    #[serde(rename = "Ready")]
    Ready,
    #[serde(rename = "Coding")]
    Coding,
    #[serde(rename = "In Review")]
    In Review,
    #[serde(rename = "Complete")]
    Complete,
}

/// A ticket's id, `SHORT-N` (upper-cased on parse).
pub type TicketId = String;

/// What the spike's methods answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SpikeEvent {
    /// A ticket, as the caller sees it.
    Ticket {
by: String,
id: TicketId,
status: Status,
    },
    /// An input answered.
    Answered {
by: String,
ticket: TicketId,
value: Status,
    },
    /// In-band refusal.
    Error {
message: String,
    },
}

// === Methods ===

/// Read one ticket.
///
/// Requires `tickets.get` anywhere it is held.
pub async fn get(client: &PlexusClient, id: TicketId) -> Result<Pin<Box<dyn Stream<Item = Result<SpikeEvent>> + Send>>> {
    let stream = client.call_stream("tickets.get", json!({ "id": id })).await?;

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
/// Requires `tickets.schema` anywhere it is held.
pub async fn schema(client: &PlexusClient) -> Result<serde_json::Value> {
    client.call_single("tickets.schema", serde_json::Value::Null).await
}

/// Move a ticket to a status.
///
/// Requires `tickets.set_status` anywhere it is held.
pub async fn set_status(client: &PlexusClient, id: TicketId, status: Status) -> Result<Pin<Box<dyn Stream<Item = Result<SpikeEvent>> + Send>>> {
    let stream = client.call_stream("tickets.set_status", json!({ "id": id, "status": status })).await?;

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

// === Requirements (AUTH-9) ===

/// What each `tickets` method needs (AUTH-9): its grant, and the param that
/// locates what it acts on. Surfacing only — the gate decides, server-side.
pub const REQUIRES: &[crate::client::Requirement] = &[
    crate::client::Requirement { method: "get", grant: "tickets.get", target: None },
    crate::client::Requirement { method: "schema", grant: "tickets.schema", target: None },
    crate::client::Requirement { method: "set_status", grant: "tickets.set_status", target: None },
];

/// The requirement of one `tickets` method, by its name or its grant.
pub fn requires_of(method: &str) -> Option<&'static crate::client::Requirement> {
    REQUIRES.iter().find(|r| r.method == method || r.grant == method)
}
