//! Hub Codegen CLI

use anyhow::Result;
use clap::{Parser, ValueEnum};
use hub_codegen::cache::{read_cache_manifest, write_cache_manifest, CodeCacheManifest, ToolchainVersions, CodePluginCache};
use hub_codegen::merge::{merge_generated_code, print_merge_summary, MergeStrategy};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CodegenTarget {
    /// Generate TypeScript client
    Typescript,
    /// Generate Rust client
    Rust,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum OutputFormat {
    #[default]
    Files,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum CliTransport {
    #[default]
    Ws,
    Browser,
    None,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum CliGenerate {
    /// All artifacts (default)
    #[default]
    All,
    /// transport.ts only
    Transport,
    /// Core RPC layer: types.ts, rpc.ts, index.ts
    Rpc,
    /// Plugin client files (<namespace>/types.ts, client.ts, index.ts)
    Plugins,
    /// Schema walk smoke test (smoke.ts, no test framework)
    Smoke,
    /// package.json only
    Package,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CodegenOutput<'a> {
    files: &'a HashMap<String, String>,
    file_hashes: &'a HashMap<String, String>,
    warnings: Vec<WarningOutput<'a>>,
    hub_codegen_version: &'static str,
    dependencies: &'a HashMap<String, String>,
    dev_dependencies: &'a HashMap<String, String>,
}

#[derive(Serialize)]
struct WarningOutput<'a> {
    location: &'a str,
    message: &'a str,
}

#[derive(Parser)]
#[command(name = "hub-codegen")]
#[command(about = "Generate client code from Synapse IR")]
#[command(version = hub_codegen::HUB_CODEGEN_VERSION)]
struct Args {
    /// Path to IR JSON file (use - for stdin)
    #[arg(default_value = "-")]
    input: PathBuf,

    /// Output directory (used in --output-format files mode)
    #[arg(short, long, default_value = "./generated")]
    output: PathBuf,

    /// Target language
    #[arg(short, long, value_enum, default_value = "typescript")]
    target: CodegenTarget,

    /// Dry run - print generated files without writing (files mode only)
    #[arg(long)]
    dry_run: bool,

    /// Transport environment: ws (Node.js/test), browser (native WebSocket, no ws import), none (external @plexus/rpc-client)
    #[arg(long, value_enum, default_value = "ws")]
    transport: CliTransport,

    /// Merge strategy for handling user-modified files (files mode only)
    #[arg(long, default_value = "skip")]
    merge_strategy: MergeStrategy,

    /// Output format: files (write to --output dir) or json (emit JSON to stdout)
    #[arg(long, value_enum, default_value = "files")]
    output_format: OutputFormat,

    /// Generate selector: which artifact subset to produce
    #[arg(long, value_enum, default_value = "all")]
    generate: CliGenerate,

    /// Plugin name filter for --generate plugins (comma-separated, e.g. "echo,health")
    #[arg(long)]
    plugins: Option<String>,

    /// Transport import path used in --generate smoke (default: ../transport)
    #[arg(long, default_value = "../transport")]
    smoke_transport_path: String,

    /// Backend WebSocket URL embedded as fallback in generated smoke tests
    #[arg(long, default_value = "ws://localhost:4444")]
    backend_url: String,

    /// IR-7: Exit with a non-zero status after writing files when generated
    /// code consumes any deprecated IR surface. stderr still carries one
    /// `WARNING:` line per deprecated consumption.
    #[arg(long)]
    fail_on_deprecated: bool,

    /// IR-7: Suppress deprecation annotations and the associated stderr
    /// warnings. Codegen behaves as if the IR were pre-IR.
    #[arg(long)]
    no_deprecation_annotations: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Read IR — transport-only generation doesn't need IR (transport.ts is a static template)
    let ir: hub_codegen::IR = if matches!(args.generate, CliGenerate::Transport) {
        serde_json::from_str(r#"{"irVersion":"2.0","irBackend":"","irTypes":{},"irMethods":{},"irPlugins":{}}"#)?
    } else if args.input.as_os_str() == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        serde_json::from_str(&buf)?
    } else {
        serde_json::from_str(&std::fs::read_to_string(&args.input)?)?
    };

    let deprecation_opts = hub_codegen::deprecation::DeprecationOptions {
        enabled: !args.no_deprecation_annotations,
    };

    // Create generation options
    let options = hub_codegen::GenerationOptions {
        transport: match args.transport {
            CliTransport::Ws      => hub_codegen::generator::TransportEnv::Ws,
            CliTransport::Browser => hub_codegen::generator::TransportEnv::Browser,
            CliTransport::None    => hub_codegen::generator::TransportEnv::None,
        },
        generate: match args.generate {
            CliGenerate::All       => hub_codegen::GenerateSelector::All,
            CliGenerate::Transport => hub_codegen::GenerateSelector::Transport,
            CliGenerate::Rpc       => hub_codegen::GenerateSelector::Rpc,
            CliGenerate::Plugins   => hub_codegen::GenerateSelector::Plugins,
            CliGenerate::Smoke     => hub_codegen::GenerateSelector::Smoke,
            CliGenerate::Package   => hub_codegen::GenerateSelector::Package,
        },
        plugins_filter: args.plugins.map(|p| {
            p.split(',').map(|s| s.trim().to_string()).collect()
        }),
        smoke_transport_path: args.smoke_transport_path,
        backend_url: args.backend_url,
        deprecation: deprecation_opts,
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
        CodegenTarget::Rust => hub_codegen::generator::rust::generate_with_options(&ir, deprecation_opts)?,
        #[cfg(not(feature = "rust"))]
        CodegenTarget::Rust => {
            anyhow::bail!("Rust codegen not enabled. Rebuild with --features rust");
        }
    };

    // IR-7: emit deprecation warnings to stderr (one line per deprecated
    // consumption). Always printed when the schema-level warnings list is
    // populated — suppression is handled earlier by the options toggle.
    for dw in &result.deprecation_warnings {
        eprintln!("{}", dw.format_stderr());
    }

    // Print warnings to stderr
    if !result.warnings.is_empty() {
        eprintln!("\n⚠️  {} warning(s):", result.warnings.len());
        for warning in &result.warnings {
            eprintln!("   {} - {}", warning.location, warning.message);
        }
        eprintln!();
    }

    match args.output_format {
        OutputFormat::Json => {
            let out = CodegenOutput {
                files: &result.files,
                file_hashes: &result.file_hashes,
                warnings: result.warnings.iter().map(|w| WarningOutput {
                    location: &w.location,
                    message: &w.message,
                }).collect(),
                hub_codegen_version: hub_codegen::HUB_CODEGEN_VERSION,
                dependencies: &result.dependencies,
                dev_dependencies: &result.dev_dependencies,
            };
            println!("{}", serde_json::to_string(&out)?);
        }
        OutputFormat::Files => {
            if args.dry_run {
                for (path, content) in &result.files {
                    println!("=== {} ===", path);
                    println!("{}", content);
                    println!();
                }
            } else {
                // Create output directory if it doesn't exist
                std::fs::create_dir_all(&args.output)?;

                // Write starter package.json if not already present (not in the files map,
                // so it is not subject to three-way merge — the user owns it after first run)
                #[cfg(feature = "typescript")]
                {
                    let pkg_path = args.output.join("package.json");
                    if !pkg_path.exists() {
                        let has_bidir = hub_codegen::generator::typescript::tests::has_bidir_methods(&ir);
                        // Version hash from code files only (exclude package.json and metadata sidecar)
                        let code_files: std::collections::HashMap<_, _> = result.files.iter()
                            .filter(|(k, _)| k.as_str() != "package.json" && k.as_str() != ".codegen-metadata.json")
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        let version_hash = hub_codegen::hash::compute_plugin_hash(&code_files);
                        let pkg_content = hub_codegen::generator::typescript::package::generate_package_json(
                            options.transport,
                            has_bidir,
                            &version_hash,
                        );
                        std::fs::write(&pkg_path, &pkg_content)?;
                    }
                }

                // Determine backend name from IR (or use default)
                let backend = ir.ir_backend.clone();
                let target_name = match args.target {
                    CodegenTarget::Typescript => "typescript",
                    CodegenTarget::Rust => "rust",
                };

                // Try to read existing cache manifest.
                // We keep the cache even on version mismatch so the three-way merge can
                // still detect user modifications (cached != current → skip).
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
                    synapse_cc: "0.0.0".to_string(),
                    synapse: "0.0.0".to_string(),
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
                            updated_file_hashes.insert(skipped_path.to_string(), old_hash.clone());
                        }
                    }
                }

                let cache_entry = CodePluginCache {
                    ir_hash: ir.ir_hash.clone().unwrap_or_default(),
                    file_hashes: updated_file_hashes,
                    cached_at: String::new(),
                };

                manifest.plugins.clear();
                manifest.plugins.insert("default".to_string(), cache_entry);

                // Write updated cache manifest
                write_cache_manifest(target_name, &backend, &manifest)?;
            }
        }
    }

    // IR-7: Escalate to non-zero exit after files are written when
    // --fail-on-deprecated is set and any deprecated surface was consumed.
    if args.fail_on_deprecated && !result.deprecation_warnings.is_empty() {
        std::process::exit(2);
    }

    Ok(())
}
