//! Hub Codegen - TypeScript code generator from Synapse IR

pub mod ir;
pub mod generator;

pub use ir::IR;
pub use generator::{generate, GenerationResult, Warning};
