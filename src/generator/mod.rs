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
    /// Map of relative path -> content hash (for cache invalidation)
    pub file_hashes: HashMap<String, String>,
    /// Runtime npm dependencies (name -> version range)
    pub dependencies: HashMap<String, String>,
    /// npm dev dependencies (name -> version range)
    pub dev_dependencies: HashMap<String, String>,
}

/// Transport environment for generated code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportEnv { Ws, Browser, None }

/// Options for code generation
#[derive(Debug, Clone)]
pub struct GenerationOptions {
    pub transport: TransportEnv,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self { transport: TransportEnv::Ws }
    }
}

