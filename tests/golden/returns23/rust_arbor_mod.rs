//! Module for arbor namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

// Child namespaces
pub mod tree;

// === Types ===

#[deprecated(since = "0.5", note = "use Node (removed in 0.8)")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyNode {
    pub id: String,
}

/// A node in the tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub label: Option<String>,
    pub payload: serde_json::Value,
    #[deprecated(since = "0.5", note = "use label (removed in 0.8)")]
    #[serde(rename = "oldLabel")]
    pub old_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeKind {
    #[serde(rename = "root")]
    Root,
    #[serde(rename = "branch")]
    Branch,
    #[serde(rename = "leaf")]
    Leaf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TreeEvent {
    #[serde(rename = "created")]
    Created {
node: Node,
    },
    #[serde(rename = "removed")]
    Removed {
id: String,
    },
    #[serde(rename = "annotated")]
    Annotated {
note: serde_json::Value,
    },
}

// === Methods ===

/// Create a node
pub async fn create(client: &PlexusClient, label: String, kind: NodeKind) -> Result<Node> {
    client.call_single("arbor.create", json!({ "label": label, "kind": kind })).await
}

/// Call arbor.delete
pub async fn delete(client: &PlexusClient, id: String) -> Result<bool> {
    client.call_single("arbor.delete", json!({ "id": id })).await
}

/// Call arbor.get
pub async fn get(client: &PlexusClient, id: String, opts: serde_json::Value) -> Result<Node> {
    client.call_single("arbor.get", json!({ "id": id, "opts": opts })).await
}

/// Call arbor.list
pub async fn list(client: &PlexusClient) -> Result<Vec<Node>> {
    client.call_single("arbor.list", serde_json::Value::Null).await
}

// DEPRECATED param `force`: since 0.5, removed in 0.8: renames always apply
/// Call arbor.rename
pub async fn rename(client: &PlexusClient, id: String, label: String, force: bool) -> Result<Node> {
    client.call_single("arbor.rename", json!({ "id": id, "label": label, "force": force })).await
}

/// Call arbor.watch
pub async fn watch(client: &PlexusClient) -> Result<Pin<Box<dyn Stream<Item = Result<TreeEvent>> + Send>>> {
    let stream = client.call_stream("arbor.watch", serde_json::Value::Null).await?;

    // Filter and transform stream items to typed data
    let typed_stream = stream.filter_map(|item| async move {
        match item {
            Ok(PlexusStreamItem::Data { content, .. }) => {
                match serde_json::from_value::<TreeEvent>(content) {
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

#[deprecated(since = "0.5", note = "use delete (removed in 0.8)")]
/// Call arbor.zap
pub async fn zap(client: &PlexusClient, id: String) -> Result<serde_json::Value> {
    client.call_single("arbor.zap", json!({ "id": id })).await
}
