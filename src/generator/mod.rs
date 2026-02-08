//! Code generation from IR

#[cfg(feature = "typescript")]
pub mod typescript;

#[cfg(feature = "rust")]
pub mod rust;

use std::collections::HashMap;

/// A warning emitted during code generation
#[derive(Debug, Clone)]
pub struct Warning {
    pub location: String,
    pub message: String,
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

/// Result of code generation
pub struct GenerationResult {
    /// Map of relative path -> file content
    pub files: HashMap<String, String>,
    /// Warnings encountered during generation
    pub warnings: Vec<Warning>,
}

/// Options for code generation
#[derive(Debug, Clone)]
pub struct GenerationOptions {
    /// Whether to bundle transport code (for TypeScript)
    /// If false, assumes external @plexus/rpc-client package
    pub bundle_transport: bool,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            bundle_transport: true,
        }
    }
}

