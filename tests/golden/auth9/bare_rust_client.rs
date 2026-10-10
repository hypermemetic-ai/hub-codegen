//! Auto-generated Plexus client
//! Do not edit manually

use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde_json::json;
use std::pin::Pin;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::SinkExt;

/// Plexus WebSocket client
#[derive(Clone)]
pub struct PlexusClient {
    url: String,
}

impl PlexusClient {
    /// Create a new client
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    /// Call a streaming method and return a stream of PlexusStreamItems
    pub(crate) async fn call_stream(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<PlexusStreamItem>> + Send>>> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Send JSON-RPC request
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        write
            .send(Message::Text(request.to_string()))
            .await?;

        // Return stream that processes responses
        let stream = async_stream::stream! {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Parse as JSON-RPC response
                        if let Ok(rpc_response) = serde_json::from_str::<serde_json::Value>(&text) {
                            // Extract the result field which contains the PlexusStreamItem
                            if let Some(result) = rpc_response.get("result") {
                                match serde_json::from_value::<PlexusStreamItem>(result.clone()) {
                                    Ok(item) => {
                                        let is_done = matches!(item, PlexusStreamItem::Done { .. });
                                        let is_error = matches!(item, PlexusStreamItem::Error { .. });

                                        yield Ok(item);

                                        // Stop stream on done or error
                                        if is_done || is_error {
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        yield Err(anyhow!("Failed to parse PlexusStreamItem: {}", e));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        yield Err(e.into());
                        break;
                    }
                    _ => continue,
                }
            }
        };

        Ok(Box::pin(stream))
    }

    /// Call a non-streaming method and return a single result
    pub(crate) async fn call_single<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T> {
        let mut stream = self.call_stream(method, params).await?;

        // Collect all data items
        while let Some(item) = stream.next().await {
            let item = item?;
            match item {
                PlexusStreamItem::Data { content, content_type, .. } => {
                    // Try to deserialize the content
                    return serde_json::from_value(content)
                        .map_err(|e| anyhow!("Failed to deserialize {}: {}", content_type, e));
                }
                PlexusStreamItem::Error { message, code, .. } => {
                    return Err(anyhow!("Plexus error{}: {}",
                        code.map(|c| format!(" [{}]", c)).unwrap_or_default(),
                        message
                    ));
                }
                PlexusStreamItem::Done { .. } => {
                    return Err(anyhow!("Stream completed without data"));
                }
                PlexusStreamItem::Progress { .. } => {
                    // Skip progress items
                    continue;
                }
            }
        }

        Err(anyhow!("Stream ended without result"))
    }
}
