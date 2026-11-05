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

// === Method requirements (AUTH-9) ===
// Surfacing only — the gate decides, server-side.

/// What a method needs (AUTH-9): its grant, and the param that locates what
/// a call acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requirement {
    /// The method's name within its activation, e.g. `"set_status"`.
    pub method: &'static str,
    /// The grant that allows it: `<activation>.<method>`, e.g. `"tickets.set_status"`.
    pub grant: &'static str,
    /// The param whose value locates what the call acts on; the grant must
    /// be held on that thing's scope. `None`: allowed wherever it is held.
    pub target: Option<&'static str>,
}

// === BIDIR-19: the input half (plexus-wire spec/10) ===

/// The service's namespace: what `{ns}.call`, `{ns}.input` and `{ns}.end` are named by.
const SERVICE_NAMESPACE: &str = "perf5";

/// One item of a call's input half.
pub enum InputItem {
    /// An item as JSON, in a `{ns}.input` frame.
    Json(serde_json::Value),
    /// An `Input<Bytes>` item: one WebSocket binary frame.
    Binary(Vec<u8>),
}

/// What a method with an input half sends: an item, or why it could not be made one.
pub type InputStream = Pin<Box<dyn Stream<Item = Result<InputItem>> + Send>>;

impl PlexusClient {
    /// A stream of items as JSON input.
    pub fn json_input<T: serde::Serialize>(input: impl Stream<Item = T> + Send + 'static) -> InputStream {
        Box::pin(input.map(|item| {
            serde_json::to_value(&item)
                .map(InputItem::Json)
                .map_err(|e| anyhow!("an input item will not serialize: {}", e))
        }))
    }

    /// A stream of byte chunks as binary input, one frame a chunk.
    pub fn binary_input<B: AsRef<[u8]>>(input: impl Stream<Item = B> + Send + 'static) -> InputStream {
        Box::pin(input.map(|chunk| Ok(InputItem::Binary(chunk.as_ref().to_vec()))))
    }

    /// Call a method with an input half: `input` is sent as the call runs,
    /// within the credit the service grants, and ended when it ends.
    pub(crate) async fn call_stream_with_input(
        &self,
        method: &str,
        params: serde_json::Value,
        mut input: InputStream,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<PlexusStreamItem>> + Send>>> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (mut write, mut read) = ws_stream.split();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": format!("{}.call", SERVICE_NAMESPACE),
            "params": { "method": method, "params": params },
        });
        write.send(Message::Text(request.to_string())).await?;

        let stream = async_stream::stream! {
            let mut subscription: Option<u64> = None;
            let mut credit: u64 = 0;
            let mut seq: u64 = 0;
            let mut open = true;
            loop {
                let feeding = open && credit > 0 && subscription.is_some();
                tokio::select! {
                    biased;
                    msg = read.next() => {
                        let text = match msg {
                            Some(Ok(Message::Text(text))) => text,
                            Some(Ok(Message::Close(_))) | None => {
                                yield Err(anyhow!("the connection closed under {} after {} input frames; nothing was resent", method_name(&request), seq));
                                break;
                            }
                            Some(Ok(_)) => continue,
                            Some(Err(e)) => {
                                yield Err(e.into());
                                break;
                            }
                        };
                        let Ok(frame) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                        if frame.get("id").and_then(|i| i.as_u64()) == Some(1) {
                            if let Some(err) = frame.get("error") {
                                yield Err(anyhow!("Plexus error: {}", err));
                                break;
                            }
                            subscription = frame.get("result").and_then(|r| r.as_u64());
                            continue;
                        }
                        let Some(result) = frame.get("params").and_then(|p| p.get("result")) else { continue };
                        match result.get("type").and_then(|t| t.as_str()) {
                            Some("credit") => credit += result.get("n").and_then(|n| n.as_u64()).unwrap_or(0),
                            Some("ack") => {}
                            Some("input_closed") => open = false,
                            _ => match serde_json::from_value::<PlexusStreamItem>(result.clone()) {
                                Ok(item) => {
                                    let last = matches!(item, PlexusStreamItem::Done { .. } | PlexusStreamItem::Error { .. });
                                    yield Ok(item);
                                    if last {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    yield Err(anyhow!("Failed to parse PlexusStreamItem: {}", e));
                                    break;
                                }
                            },
                        }
                    }
                    item = input.next(), if feeding => {
                        let sub = subscription.expect("feeding needs the subscription");
                        let sent = match item {
                            Some(Err(e)) => {
                                // nothing is sent for it: the call ends here, said locally
                                yield Err(e);
                                break;
                            }
                            Some(Ok(InputItem::Json(item))) => {
                                let frame = json!({
                                    "jsonrpc": "2.0",
                                    "method": format!("{}.input", SERVICE_NAMESPACE),
                                    "params": { "subscription": sub, "seq": seq, "items": [item] },
                                });
                                write.send(Message::Text(frame.to_string())).await
                            }
                            Some(Ok(InputItem::Binary(payload))) => {
                                let mut frame = Vec::with_capacity(17 + payload.len());
                                frame.push(0x01);
                                frame.extend_from_slice(&sub.to_be_bytes());
                                frame.extend_from_slice(&seq.to_be_bytes());
                                frame.extend_from_slice(&payload);
                                write.send(Message::Binary(frame)).await
                            }
                            None => {
                                open = false;
                                let frame = json!({
                                    "jsonrpc": "2.0",
                                    "method": format!("{}.end", SERVICE_NAMESPACE),
                                    "params": { "subscription": sub, "seq": seq },
                                });
                                if let Err(e) = write.send(Message::Text(frame.to_string())).await {
                                    yield Err(e.into());
                                    break;
                                }
                                continue;
                            }
                        };
                        if let Err(e) = sent {
                            yield Err(anyhow!("the connection closed under {} after {} input frames; nothing was resent: {}", method_name(&request), seq, e));
                            break;
                        }
                        seq += 1;
                        credit -= 1;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    /// [`Self::call_single`], for a method with an input half.
    pub(crate) async fn call_single_with_input<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
        input: InputStream,
    ) -> Result<T> {
        let mut stream = self.call_stream_with_input(method, params, input).await?;
        while let Some(item) = stream.next().await {
            match item? {
                PlexusStreamItem::Data { content, content_type, .. } => {
                    return serde_json::from_value(content)
                        .map_err(|e| anyhow!("Failed to deserialize {}: {}", content_type, e));
                }
                PlexusStreamItem::Error { message, code, .. } => {
                    return Err(anyhow!("Plexus error{}: {}",
                        code.map(|c| format!(" [{}]", c)).unwrap_or_default(),
                        message
                    ));
                }
                PlexusStreamItem::Done { .. } => return Err(anyhow!("Stream completed without data")),
                PlexusStreamItem::Progress { .. } => continue,
            }
        }
        Err(anyhow!("Stream ended without result"))
    }
}

fn method_name(request: &serde_json::Value) -> String {
    request["params"]["method"].as_str().unwrap_or("the call").to_string()
}
