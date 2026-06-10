//! Module for cone namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

// === Methods ===

/// Credential-requirement metadata for `cone.send_message` (R-4). Surfacing only.
pub const SEND_MESSAGE_AUTH: crate::client::MethodAuthMetadata = crate::client::MethodAuthMetadata {
    requires_credential: Some(crate::client::CredentialRequirement { kind: Some("oauth_access"), scopes: &["facet.write", "facet.read"], site_hint: Some("header:authorization") }),
    public: false,
    auth_posture: Some(crate::client::AuthPosture::Required),
};

/// Send a message
///
/// Requires credential — kind: oauth_access, scopes: [facet.write, facet.read], site: header:authorization
/// Auth posture: required
pub async fn send_message(client: &PlexusClient) -> Result<String> {
    client.call_single("cone.send_message", serde_json::Value::Null).await
}
