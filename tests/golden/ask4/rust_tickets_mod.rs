//! Module for tickets namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

// === Types ===

/// What tickets.archive streams
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ArchiveEvent {
    /// One ticket archived
    #[serde(rename = "archived")]
    Archived {
id: String,
    },
}

/// What tickets.delete answers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deleted {
    pub ids: Vec<String>,
}

// === Methods ===

/// Archive the tickets a filter matches
///
/// Requires `tickets.archive` anywhere it is held.
pub async fn archive(client: &PlexusClient, filter: String, confirm: Option<ConfirmToken>) -> Result<Pin<Box<dyn Stream<Item = Result<Asking<ArchiveEvent>>> + Send>>> {
    let stream = client.call_stream("tickets.archive", json!({ "filter": filter, "confirm": confirm })).await?;

    // Filter and transform stream items to typed data
    let typed_stream = stream.filter_map(|item| async move {
        match item {
            Ok(PlexusStreamItem::Data { content, .. }) => {
                match serde_json::from_value::<Asking<ArchiveEvent>>(content) {
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

/// Delete tickets for good
///
/// Requires `tickets.delete` anywhere it is held.
pub async fn delete(client: &PlexusClient, ids: Vec<String>, children: String, confirm: Option<ConfirmToken>) -> Result<Asking<Deleted>> {
    client.call_single("tickets.delete", json!({ "ids": ids, "children": children, "confirm": confirm })).await
}

/// One ticket (asks nothing)
///
/// Requires `tickets.get` anywhere it is held.
pub async fn get(client: &PlexusClient, id: String) -> Result<Deleted> {
    client.call_single("tickets.get", json!({ "id": id })).await
}

// === Requirements (AUTH-9) ===

/// What each `tickets` method needs (AUTH-9): its grant, and the param that
/// locates what it acts on. Surfacing only — the gate decides, server-side.
pub const REQUIRES: &[crate::client::Requirement] = &[
    crate::client::Requirement { method: "archive", grant: "tickets.archive", target: None },
    crate::client::Requirement { method: "delete", grant: "tickets.delete", target: None },
    crate::client::Requirement { method: "get", grant: "tickets.get", target: None },
];

/// The requirement of one `tickets` method, by its name or its grant.
pub fn requires_of(method: &str) -> Option<&'static crate::client::Requirement> {
    REQUIRES.iter().find(|r| r.method == method || r.grant == method)
}
