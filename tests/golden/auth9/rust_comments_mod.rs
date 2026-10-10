//! Module for comments namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

// === Methods ===

/// Comments, newest first
///
/// Requires `comments.list` anywhere it is held.
pub async fn list(client: &PlexusClient) -> Result<String> {
    client.call_single("comments.list", serde_json::Value::Null).await
}

// === Requirements (AUTH-9) ===

/// What each `comments` method needs (AUTH-9): its grant, and the param that
/// locates what it acts on. Surfacing only — the gate decides, server-side.
pub const REQUIRES: &[crate::client::Requirement] = &[
    crate::client::Requirement { method: "list", grant: "comments.list", target: None },
];

/// The requirement of one `comments` method, by its name or its grant.
pub fn requires_of(method: &str) -> Option<&'static crate::client::Requirement> {
    REQUIRES.iter().find(|r| r.method == method || r.grant == method)
}
