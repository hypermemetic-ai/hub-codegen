//! Hub Codegen CLI

use anyhow::Result;
use clap::Parser;
use std::io::{self, Read};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "hub-codegen")]
#[command(about = "Generate TypeScript client from Synapse IR")]
struct Args {
    /// Path to IR JSON file (use - for stdin)
    #[arg(default_value = "-")]
    input: PathBuf,

    /// Output directory
    #[arg(short, long, default_value = "./generated")]
    output: PathBuf,

    /// Dry run - print generated files without writing
    #[arg(long)]
    dry_run: bool,
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

    // Generate
    let result = hub_codegen::generate(&ir)?;

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
