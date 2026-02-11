//! Hub Codegen CLI

use anyhow::Result;
use clap::{Parser, ValueEnum};
use hub_codegen::cache::{read_cache_manifest, write_cache_manifest, CodeCacheManifest, ToolchainVersions, CodePluginCache};
use hub_codegen::merge::{merge_generated_code, print_merge_summary, MergeStrategy};
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

    /// Merge strategy for handling user-modified files (skip|force|interactive)
    #[arg(long, default_value = "skip")]
    merge_strategy: MergeStrategy,
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
        // Create output directory if it doesn't exist
        std::fs::create_dir_all(&args.output)?;

        // Determine backend name from IR (or use default)
        let backend = ir.ir_backend.clone();
        let target_name = match args.target {
            CodegenTarget::Typescript => "typescript",
            CodegenTarget::Rust => "rust",
        };

        // Try to read existing cache manifest
        let cache_manifest = read_cache_manifest(target_name, &backend).ok();

        // Perform three-way merge
        let merge_result = merge_generated_code(
            &result.files,
            &args.output,
            cache_manifest.as_ref(),
            args.merge_strategy,
        )?;

        // Print merge summary with warnings
        print_merge_summary(&merge_result);

        // Update cache manifest with new file hashes
        let toolchain = ToolchainVersions {
            synapse_cc: "0.0.0".to_string(), // Will be populated by synapse-cc
            synapse: "0.0.0".to_string(),    // Will be populated by synapse-cc
            hub_codegen: hub_codegen::HUB_CODEGEN_VERSION.to_string(),
        };

        let mut manifest = cache_manifest.unwrap_or_else(|| {
            CodeCacheManifest::new(target_name.to_string(), toolchain.clone())
        });

        // Update toolchain version
        manifest.toolchain = toolchain;

        // Build file hashes for cache: use new hashes for written files,
        // preserve old hashes for skipped files
        let mut updated_file_hashes = result.file_hashes.clone();

        // For skipped files, restore the old hash from cache to preserve conflict detection
        if let Some(old_plugin) = manifest.plugins.get("default") {
            for skipped_file in &merge_result.skipped {
                let skipped_path = skipped_file.to_str().unwrap();
                if let Some(old_hash) = old_plugin.file_hashes.get(skipped_path) {
                    // Keep the old hash so future runs can detect the conflict
                    updated_file_hashes.insert(skipped_path.to_string(), old_hash.clone());
                }
            }
        }

        let cache_entry = CodePluginCache {
            ir_hash: ir.ir_hash.clone().unwrap_or_default(),
            file_hashes: updated_file_hashes,
            cached_at: String::new(), // Will be set by add_plugin
        };

        manifest.plugins.clear();
        manifest.plugins.insert("default".to_string(), cache_entry);

        // Write updated cache manifest
        write_cache_manifest(target_name, &backend, &manifest)?;
    }

    Ok(())
}
