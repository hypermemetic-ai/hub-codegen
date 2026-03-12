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

/// Selector for which artifacts to generate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenerateSelector {
    /// Generate all artifacts (default behaviour)
    #[default]
    All,
    /// transport.ts only
    Transport,
    /// Core RPC layer: types.ts, rpc.ts, index.ts
    Rpc,
    /// Plugin client files: <namespace>/types.ts, <namespace>/client.ts, <namespace>/index.ts
    Plugins,
    /// Schema walk smoke test script (no test framework)
    Smoke,
    /// package.json only
    Package,
}

/// Options for code generation
#[derive(Debug, Clone)]
pub struct GenerationOptions {
    pub transport: TransportEnv,
    /// Which artifact subset to produce
    pub generate: GenerateSelector,
    /// Optional plugin name filter for GenPlugins (None = all plugins)
    pub plugins_filter: Option<Vec<String>>,
    /// Import path for PlexusRpcClient in the smoke test (default: "../transport")
    pub smoke_transport_path: String,
    /// Backend WebSocket URL used as fallback in generated smoke tests
    pub backend_url: String,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            transport: TransportEnv::Ws,
            generate: GenerateSelector::All,
            plugins_filter: None,
            smoke_transport_path: "../transport".to_string(),
            backend_url: "ws://localhost:4444".to_string(),
        }
    }
}

