//! Service Codegen - Multi-language code generator from Synapse IR

/// service-codegen version (from Cargo.toml)
pub const SERVICE_CODEGEN_VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod ir;
pub mod generator;
pub mod hash;
pub mod cache;
pub mod merge;
pub mod deprecation;
pub mod credential_surface;
pub mod grant_surface;
pub mod service_types;

pub use ir::IR;
pub use generator::{GenerationResult, Warning, GenerationOptions, TransportEnv, GenerateSelector};

// Conditionally export generators based on features
#[cfg(feature = "typescript")]
pub use generator::typescript::generate as generate_typescript;

#[cfg(feature = "rust")]
pub use generator::rust::generate as generate_rust;

// Legacy alias for TypeScript generation (default when typescript feature is enabled)
#[cfg(feature = "typescript")]
pub use generator::typescript::generate as generate;
