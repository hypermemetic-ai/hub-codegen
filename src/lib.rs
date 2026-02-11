//! Hub Codegen - Multi-language code generator from Synapse IR

/// hub-codegen version (from Cargo.toml)
pub const HUB_CODEGEN_VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod ir;
pub mod generator;
pub mod hash;
pub mod cache;
pub mod merge;

pub use ir::IR;
pub use generator::{GenerationResult, Warning, GenerationOptions};

// Conditionally export generators based on features
#[cfg(feature = "typescript")]
pub use generator::typescript::generate as generate_typescript;

#[cfg(feature = "rust")]
pub use generator::rust::generate as generate_rust;

// Legacy alias for TypeScript generation (default when typescript feature is enabled)
#[cfg(feature = "typescript")]
pub use generator::typescript::generate as generate;
