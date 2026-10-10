//! Module for tickets namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

// === Methods ===

/// Change a ticket's status
///
/// Moves it through Pending → Ready → Coding → In Review → Complete;
/// a doc has no lifecycle and is refused.
///
/// Requires `tickets.set_status` on the scope of `id`.
pub async fn set_status(client: &PlexusClient, id: String, status: String) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
    let stream = client.call_stream("tickets.set_status", json!({ "id": id, "status": status })).await?;

    // Filter and transform stream items to typed data
    let typed_stream = stream.filter_map(|item| async move {
        match item {
            Ok(PlexusStreamItem::Data { content, .. }) => {
                match serde_json::from_value::<String>(content) {
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
    crate::client::Requirement { method: "set_status", grant: "tickets.set_status", target: Some("id") },
];

/// The requirement of one `tickets` method, by its name or its grant.
pub fn requires_of(method: &str) -> Option<&'static crate::client::Requirement> {
    REQUIRES.iter().find(|r| r.method == method || r.grant == method)
}

/// The `tickets` activation's default roles, each the grants it lists (AUTH-3).
/// Defaults only: a server's roles are data an admin may change, and
/// `admin` holds every grant.
pub const DEFAULT_ROLES: &[(&str, &[&str])] = &[
    ("editor", &["tickets.get", "tickets.set_status"]),
    ("viewer", &["tickets.get"]),
];
