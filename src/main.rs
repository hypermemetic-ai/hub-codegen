//! Hub Codegen CLI

use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::io::{self, Read};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CodegenTarget {
    /// Generate TypeScript client
    Typescript,
    /// Generate Rust client
    Rust,
}

#[derive(Parser)]
#[command(name = "hub-codegen")]
#[command(about = "Generate client code from Synapse IR")]
struct Args {
    /// Path to IR JSON file (use - for stdin)
    #[arg(default_value = "-")]
    input: PathBuf,

    /// Output directory
    #[arg(short, long, default_value = "./generated")]
    output: PathBuf,

    /// Target language
    #[arg(short, long, value_enum, default_value = "typescript")]
    target: CodegenTarget,

    /// Dry run - print generated files without writing
    #[arg(long)]
    dry_run: bool,

    /// Bundle transport code (default: true). If false, assumes external @plexus/rpc-client package
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    bundle_transport: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Read IR
    let ir_json = if args.input.as_os_str() == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(&args.input)?
    };

    let ir: hub_codegen::IR = serde_json::from_str(&ir_json)?;

    // Create generation options
    let options = hub_codegen::GenerationOptions {
        bundle_transport: args.bundle_transport,
    };

    // Generate based on target
    let result = match args.target {
        #[cfg(feature = "typescript")]
        CodegenTarget::Typescript => hub_codegen::generate_typescript(&ir, &options)?,
        #[cfg(not(feature = "typescript"))]
        CodegenTarget::Typescript => {
            anyhow::bail!("TypeScript codegen not enabled. Rebuild with --features typescript");
        }

        #[cfg(feature = "rust")]
        CodegenTarget::Rust => hub_codegen::generate_rust(&ir)?,
        #[cfg(not(feature = "rust"))]
        CodegenTarget::Rust => {
            anyhow::bail!("Rust codegen not enabled. Rebuild with --features rust");
        }
    };

    // Print warnings to stderr
    if !result.warnings.is_empty() {
        eprintln!("\n⚠️  {} warning(s):", result.warnings.len());
        for warning in &result.warnings {
            eprintln!("   {} - {}", warning.location, warning.message);
        }
        eprintln!();
    }

    if args.dry_run {
        for (path, content) in &result.files {
            println!("=== {} ===", path);
            println!("{}", content);
            println!();
        }
    } else {
        std::fs::create_dir_all(&args.output)?;
        for (path, content) in &result.files {
            let full_path = args.output.join(path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full_path, content)?;
            println!("Wrote: {}", full_path.display());
        }
    }

    Ok(())
}
