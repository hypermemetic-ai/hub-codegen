//! Integration test that generates Rust code and verifies it compiles

use hub_codegen::{generate_rust, IR};
use std::collections::HashMap;
use std::fs;
use std::process::Command;

fn create_comprehensive_test_ir() -> IR {
    use hub_codegen::ir::*;

    let mut ir_types = HashMap::new();
    let mut ir_methods = HashMap::new();
    let mut ir_plugins = HashMap::new();

    // ===== Create types =====

    // Simple struct
    ir_types.insert(
        "echo.Message".to_string(),
        TypeDef {
            td_name: "Message".to_string(),
            td_namespace: "echo".to_string(),
            td_description: Some("A simple message".to_string()),
            td_kind: TypeKind::KindStruct {
                ks_fields: vec![
                    FieldDef {
                        fd_name: "text".to_string(),
                        fd_type: TypeRef::RefPrimitive("string".to_string(), None),
                        fd_description: Some("Message text".to_string()),
                        fd_required: true,
                        fd_default: None,
                    },
                    FieldDef {
                        fd_name: "count".to_string(),
                        fd_type: TypeRef::RefPrimitive("integer".to_string(), Some("int64".to_string())),
                        fd_description: Some("Repeat count".to_string()),
                        fd_required: false,
                        fd_default: Some(serde_json::json!(1)),
                    },
                ],
            },
        },
    );

    // Enum with variants
    ir_types.insert(
        "echo.EchoEvent".to_string(),
        TypeDef {
            td_name: "EchoEvent".to_string(),
            td_namespace: "echo".to_string(),
            td_description: Some("Echo operation event".to_string()),
            td_kind: TypeKind::KindEnum {
                ke_discriminator: "type".to_string(),
                ke_variants: vec![
                    VariantDef {
                        vd_name: "started".to_string(),
                        vd_description: Some("Echo started".to_string()),
                        vd_fields: vec![],
                    },
                    VariantDef {
                        vd_name: "echoed".to_string(),
                        vd_description: Some("Message echoed".to_string()),
                        vd_fields: vec![
                            FieldDef {
                                fd_name: "message".to_string(),
                                fd_type: TypeRef::RefNamed(QualifiedName {
                                    qn_namespace: "echo".to_string(),
                                    qn_local_name: "Message".to_string(),
                                }),
                                fd_description: Some("The echoed message".to_string()),
                                fd_required: true,
                                fd_default: None,
                            },
                            FieldDef {
                                fd_name: "iteration".to_string(),
                                fd_type: TypeRef::RefPrimitive("integer".to_string(), Some("int64".to_string())),
                                fd_description: Some("Current iteration".to_string()),
                                fd_required: true,
                                fd_default: None,
                            },
                        ],
                    },
                    VariantDef {
                        vd_name: "completed".to_string(),
                        vd_description: Some("Echo completed".to_string()),
                        vd_fields: vec![FieldDef {
                            fd_name: "total".to_string(),
                            fd_type: TypeRef::RefPrimitive("integer".to_string(), Some("int64".to_string())),
                            fd_description: Some("Total echoes".to_string()),
                            fd_required: true,
                            fd_default: None,
                        }],
                    },
                ],
            },
        },
    );

    // Optional and array types
    ir_types.insert(
        "echo.EchoResponse".to_string(),
        TypeDef {
            td_name: "EchoResponse".to_string(),
            td_namespace: "echo".to_string(),
            td_description: Some("Echo response".to_string()),
            td_kind: TypeKind::KindStruct {
                ks_fields: vec![
                    FieldDef {
                        fd_name: "messages".to_string(),
                        fd_type: TypeRef::RefArray(Box::new(TypeRef::RefPrimitive("string".to_string(), None))),
                        fd_description: Some("All echoed messages".to_string()),
                        fd_required: true,
                        fd_default: None,
                    },
                    FieldDef {
                        fd_name: "error".to_string(),
                        fd_type: TypeRef::RefOptional(Box::new(TypeRef::RefPrimitive("string".to_string(), None))),
                        fd_description: Some("Error message if any".to_string()),
                        fd_required: false,
                        fd_default: None,
                    },
                ],
            },
        },
    );

    // ===== Create methods =====

    // Simple non-streaming method
    ir_methods.insert(
        "echo.once".to_string(),
        MethodDef {
            md_name: "once".to_string(),
            md_full_path: "echo.once".to_string(),
            md_namespace: "echo".to_string(),
            md_description: Some("Echo a message once".to_string()),
            md_streaming: false,
            md_params: vec![ParamDef {
                pd_name: "message".to_string(),
                pd_type: TypeRef::RefPrimitive("string".to_string(), None),
                pd_description: Some("Message to echo".to_string()),
                pd_required: true,
                pd_default: None,
            }],
            md_returns: TypeRef::RefNamed(QualifiedName {
                qn_namespace: "echo".to_string(),
                qn_local_name: "EchoResponse".to_string(),
            }),
        },
    );

    // Streaming method with multiple params
    ir_methods.insert(
        "echo.echo".to_string(),
        MethodDef {
            md_name: "echo".to_string(),
            md_full_path: "echo.echo".to_string(),
            md_namespace: "echo".to_string(),
            md_description: Some("Echo a message multiple times".to_string()),
            md_streaming: true,
            md_params: vec![
                ParamDef {
                    pd_name: "message".to_string(),
                    pd_type: TypeRef::RefPrimitive("string".to_string(), None),
                    pd_description: Some("Message to echo".to_string()),
                    pd_required: true,
                    pd_default: None,
                },
                ParamDef {
                    pd_name: "count".to_string(),
                    pd_type: TypeRef::RefPrimitive("integer".to_string(), Some("int64".to_string())),
                    pd_description: Some("Number of times to echo".to_string()),
                    pd_required: false,
                    pd_default: Some(serde_json::json!(1)),
                },
            ],
            md_returns: TypeRef::RefNamed(QualifiedName {
                qn_namespace: "echo".to_string(),
                qn_local_name: "EchoEvent".to_string(),
            }),
        },
    );

    ir_plugins.insert("echo".to_string(), vec!["once".to_string(), "echo".to_string()]);

    IR {
        ir_version: "2.0".to_string(),
        ir_backend: "test".to_string(),
        ir_hash: Some("smoke-test-hash".to_string()),
        ir_metadata: None,
        ir_types,
        ir_methods,
        ir_plugins,
    }
}

#[test]
fn test_generated_rust_compiles() {
    // Create test IR
    let ir = create_comprehensive_test_ir();

    // Generate Rust code
    let result = generate_rust(&ir).expect("Code generation should succeed");

    // Create temp directory for generated code
    let temp_dir = std::env::temp_dir().join("plexus-rust-codegen-test");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).expect("Failed to clean temp dir");
    }
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // Write generated files
    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir).expect("Failed to create src dir");

    for (path, content) in &result.files {
        // Put Cargo.toml at root, everything else in src/
        let file_path = if path == "Cargo.toml" {
            temp_dir.join(path)
        } else {
            src_dir.join(path)
        };

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent dir");
        }
        fs::write(&file_path, content).expect("Failed to write file");
        println!("Wrote: {}", file_path.display());
    }

    // Verify all expected files exist
    assert!(src_dir.join("lib.rs").exists());
    assert!(src_dir.join("types.rs").exists());
    assert!(src_dir.join("client.rs").exists());
    assert!(temp_dir.join("Cargo.toml").exists());

    // Try to compile the generated code
    println!("\n=== Checking generated Rust code ===");
    let cargo_check = Command::new("cargo")
        .arg("check")
        .arg("--manifest-path")
        .arg(temp_dir.join("Cargo.toml"))
        .output()
        .expect("Failed to run cargo check");

    println!("cargo check stdout:\n{}", String::from_utf8_lossy(&cargo_check.stdout));
    println!("cargo check stderr:\n{}", String::from_utf8_lossy(&cargo_check.stderr));

    if !cargo_check.status.success() {
        panic!("Generated Rust code failed to compile!");
    }

    println!("\n✅ Generated Rust code compiles successfully!");

    // Optional: cleanup
    // fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_generated_code_structure() {
    let ir = create_comprehensive_test_ir();
    let result = generate_rust(&ir).expect("Code generation should succeed");

    // Verify lib.rs structure
    let lib_content = result.files.get("lib.rs").expect("lib.rs should exist");
    assert!(lib_content.contains("pub mod types"));
    assert!(lib_content.contains("pub mod client"));
    assert!(lib_content.contains("pub use client::PlexusClient"));

    // Verify types.rs contains all types
    let types_content = result.files.get("types.rs").expect("types.rs should exist");
    assert!(types_content.contains("pub struct Message"));
    assert!(types_content.contains("pub enum EchoEvent"));
    assert!(types_content.contains("pub struct EchoResponse"));
    assert!(types_content.contains("pub text: String"));
    assert!(types_content.contains("pub count: i64"));

    // Verify client.rs contains methods
    let client_content = result.files.get("client.rs").expect("client.rs should exist");
    assert!(client_content.contains("pub struct PlexusClient"));
    assert!(client_content.contains("pub async fn once"));
    assert!(client_content.contains("pub async fn echo"));
    assert!(client_content.contains("message: String"));
    assert!(client_content.contains("count: i64"));

    // Verify streaming vs non-streaming signatures
    assert!(client_content.contains("-> Result<EchoResponse>"));  // non-streaming
    assert!(client_content.contains("Pin<Box<dyn Stream<Item = Result<EchoEvent>> + Send>>"));  // streaming

    // Verify Cargo.toml
    let cargo_toml = result.files.get("Cargo.toml").expect("Cargo.toml should exist");
    assert!(cargo_toml.contains("plexus-client"));
    assert!(cargo_toml.contains("serde"));
    assert!(cargo_toml.contains("tokio"));
    assert!(cargo_toml.contains("async-stream"));
    assert!(cargo_toml.contains("futures"));
    assert!(cargo_toml.contains("anyhow"));
}

#[test]
fn test_no_warnings() {
    let ir = create_comprehensive_test_ir();
    let result = generate_rust(&ir).expect("Code generation should succeed");

    // Should have no warnings
    assert!(result.warnings.is_empty(), "Should not have warnings, got: {:?}", result.warnings);
}
