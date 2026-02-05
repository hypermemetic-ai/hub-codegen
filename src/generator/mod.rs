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

