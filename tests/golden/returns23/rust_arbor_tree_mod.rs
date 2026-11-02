//! Module for arbor.tree namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

use crate::arbor::Node;

// === Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    pub nodes: Vec<Node>,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkStep {
    pub depth: i64,
    pub node: Node,
    pub hint: Vec<serde_json::Value>,
}

// === Methods ===

/// Call arbor.tree.graft
pub async fn graft(client: &PlexusClient, branch: Branch) -> Result<serde_json::Value> {
    client.call_single("arbor.tree.graft", json!({ "branch": branch })).await
}

/// Call arbor.tree.prune
pub async fn prune(client: &PlexusClient, depth: i64) -> Result<i64> {
    client.call_single("arbor.tree.prune", json!({ "depth": depth })).await
}

/// Call arbor.tree.walk
pub async fn walk(client: &PlexusClient, root: String) -> Result<Pin<Box<dyn Stream<Item = Result<WalkStep>> + Send>>> {
    let stream = client.call_stream("arbor.tree.walk", json!({ "root": root })).await?;

    // Filter and transform stream items to typed data
    let typed_stream = stream.filter_map(|item| async move {
        match item {
            Ok(PlexusStreamItem::Data { content, .. }) => {
                match serde_json::from_value::<WalkStep>(content) {
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
