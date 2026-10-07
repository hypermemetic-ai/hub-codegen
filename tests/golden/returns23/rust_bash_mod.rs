//! Module for bash namespace
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
pub struct ExecRequest {
    pub command: String,
    pub env: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Output {
    Stdout {
line: String,
    },
    Stderr {
line: String,
    },
    Exit {
code: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Signal {
    #[serde(rename = "term")]
    Term,
    #[serde(rename = "kill")]
    Kill,
    #[serde(rename = "int")]
    Int,
}

// === Methods ===

/// Call bash.execute
pub async fn execute(client: &PlexusClient, request: ExecRequest) -> Result<Pin<Box<dyn Stream<Item = Result<Output>> + Send>>> {
    let stream = client.call_stream("bash.execute", json!({ "request": request })).await?;

    // Filter and transform stream items to typed data
    let typed_stream = stream.filter_map(|item| async move {
        match item {
            Ok(PlexusStreamItem::Data { content, .. }) => {
                match serde_json::from_value::<Output>(content) {
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

/// Call bash.history
pub async fn history(client: &PlexusClient, limit: Option<i64>) -> Result<Vec<String>> {
    client.call_single("bash.history", json!({ "limit": limit })).await
}

/// Call bash.kill
pub async fn kill(client: &PlexusClient, pid: i64, signal: Signal) -> Result<bool> {
    client.call_single("bash.kill", json!({ "pid": pid, "signal": signal })).await
}

/// Call bash.status
pub async fn status(client: &PlexusClient) -> Result<serde_json::Value> {
    client.call_single("bash.status", serde_json::Value::Null).await
}
