//! Module for spinner namespace
//! Do not edit manually

use crate::client::PlexusClient;
use crate::types::*;
use anyhow::{anyhow, Result};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

// === Methods ===

/// Credential-requirement metadata for `spinner.spin` (R-4). Surfacing only.
pub const SPIN_AUTH: crate::client::MethodAuthMetadata = crate::client::MethodAuthMetadata {
    requires_credential: Some(crate::client::CredentialRequirement { kind: None, scopes: &["spinner.spin"], site_hint: Some("header:authorization") }),
    public: false,
    auth_posture: None,
};

/// Spin the fidget (requires scope spinner.spin)
///
/// Requires credential — scopes: [spinner.spin], site: header:authorization
pub async fn spin(client: &PlexusClient) -> Result<String> {
    client.call_single("spinner.spin", serde_json::Value::Null).await
}
