//! Module for echo namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

// === Methods ===

/// Credential-requirement metadata for `echo.ping` (R-4). Surfacing only.
pub const PING_AUTH: crate::client::MethodAuthMetadata = crate::client::MethodAuthMetadata {
    requires_credential: None,
    public: true,
    auth_posture: Some(crate::client::AuthPosture::Required),
};

/// Liveness check
///
/// Public — exempt from auth (no credential required)
/// Auth posture: required
pub async fn ping(client: &PlexusClient) -> Result<String> {
    client.call_single("echo.ping", serde_json::Value::Null).await
}
