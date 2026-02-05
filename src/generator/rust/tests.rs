//! Smoke tests for Rust code generation

#[cfg(test)]
mod tests {
    use crate::generator::rust;
    use crate::ir::*;
    use std::collections::HashMap;

    fn create_test_ir() -> IR {
        let mut ir_types = HashMap::new();
        let mut ir_methods = HashMap::new();
        let mut ir_plugins = HashMap::new();

        // Create a simple struct type
        ir_types.insert(
            "health.Status".to_string(),
            TypeDef {
                td_name: "Status".to_string(),
                td_namespace: "health".to_string(),
                td_description: Some("Health status response".to_string()),
                td_kind: TypeKind::KindStruct {
                    ks_fields: vec![
                        FieldDef {
                            fd_name: "healthy".to_string(),
                            fd_type: TypeRef::RefPrimitive("boolean".to_string(), None),
                            fd_description: Some("Whether the service is healthy".to_string()),
                            fd_required: true,
                            fd_default: None,
                        },
                        FieldDef {
                            fd_name: "uptime".to_string(),
                            fd_type: TypeRef::RefPrimitive("integer".to_string(), Some("int64".to_string())),
                            fd_description: Some("Uptime in seconds".to_string()),
                            fd_required: true,
                            fd_default: None,
                        },
                    ],
                },
            },
        );

        // Create an enum type
        ir_types.insert(
            "health.Event".to_string(),
            TypeDef {
                td_name: "Event".to_string(),
                td_namespace: "health".to_string(),
                td_description: Some("Health check event".to_string()),
                td_kind: TypeKind::KindEnum {
                    ke_discriminator: "type".to_string(),
                    ke_variants: vec![
                        VariantDef {
                            vd_name: "started".to_string(),
                            vd_description: Some("Check started".to_string()),
                            vd_fields: vec![],
                        },
                        VariantDef {
                            vd_name: "completed".to_string(),
                            vd_description: Some("Check completed".to_string()),
                            vd_fields: vec![FieldDef {
                                fd_name: "status".to_string(),
                                fd_type: TypeRef::RefNamed(QualifiedName {
                                    qn_namespace: "health".to_string(),
                                    qn_local_name: "Status".to_string(),
                                }),
                                fd_description: Some("Final status".to_string()),
                                fd_required: true,
                                fd_default: None,
                            }],
                        },
                    ],
                },
            },
        );

        // Create a non-streaming method
        ir_methods.insert(
            "health.check".to_string(),
            MethodDef {
                md_name: "check".to_string(),
                md_full_path: "health.check".to_string(),
                md_namespace: "health".to_string(),
                md_description: Some("Check service health".to_string()),
                md_streaming: false,
                md_params: vec![],
                md_returns: TypeRef::RefNamed(QualifiedName {
                    qn_namespace: "health".to_string(),
                    qn_local_name: "Status".to_string(),
                }),
            },
        );

        // Create a streaming method
        ir_methods.insert(
            "health.watch".to_string(),
            MethodDef {
                md_name: "watch".to_string(),
                md_full_path: "health.watch".to_string(),
                md_namespace: "health".to_string(),
                md_description: Some("Watch health status changes".to_string()),
                md_streaming: true,
                md_params: vec![ParamDef {
                    pd_name: "interval".to_string(),
                    pd_type: TypeRef::RefPrimitive("integer".to_string(), Some("int64".to_string())),
                    pd_description: Some("Interval in seconds".to_string()),
                    pd_required: false,
                    pd_default: Some(serde_json::json!(5)),
                }],
                md_returns: TypeRef::RefNamed(QualifiedName {
                    qn_namespace: "health".to_string(),
                    qn_local_name: "Event".to_string(),
                }),
            },
        );

        ir_plugins.insert("health".to_string(), vec!["check".to_string(), "watch".to_string()]);

        IR {
            ir_version: "2.0".to_string(),
            ir_hash: Some("test-hash-123".to_string()),
            ir_types,
            ir_methods,
            ir_plugins,
        }
    }

    #[test]
    fn test_generate_types() {
        let ir = create_test_ir();
        let types_content = rust::types::generate_types(&ir);

        // Should contain struct definition
        assert!(types_content.contains("pub struct Status"));
        assert!(types_content.contains("pub healthy: bool"));
        assert!(types_content.contains("pub uptime: i64"));

        // Should contain enum definition
        assert!(types_content.contains("pub enum Event"));
        assert!(types_content.contains("Started"));
        assert!(types_content.contains("Completed"));

        // Should contain core transport types
        assert!(types_content.contains("pub enum PlexusStreamItem"));
        assert!(types_content.contains("PlexusError"));
    }

    #[test]
    fn test_generate_client() {
        let ir = create_test_ir();
        let client_content = rust::client::generate_client(&ir);

        // Should contain client struct
        assert!(client_content.contains("pub struct PlexusClient"));
        assert!(client_content.contains("pub fn new("));

        // Should contain non-streaming method
        assert!(client_content.contains("pub async fn check"));
        assert!(client_content.contains("-> Result<Status>"));

        // Should contain streaming method
        assert!(client_content.contains("pub async fn watch"));
        assert!(client_content.contains("interval: i64"));
        assert!(client_content.contains("Pin<Box<dyn Stream<Item = Result<Event>> + Send>>"));
    }

    #[test]
    fn test_full_generation() {
        let ir = create_test_ir();
        let result = rust::generate(&ir).expect("Generation should succeed");

        // Should have all required files
        assert!(result.files.contains_key("lib.rs"));
        assert!(result.files.contains_key("types.rs"));
        assert!(result.files.contains_key("client.rs"));
        assert!(result.files.contains_key("Cargo.toml"));

        // lib.rs should re-export modules
        let lib_content = result.files.get("lib.rs").unwrap();
        assert!(lib_content.contains("pub mod types"));
        assert!(lib_content.contains("pub mod client"));
        assert!(lib_content.contains("pub use client::PlexusClient"));

        // Cargo.toml should have dependencies
        let cargo_toml = result.files.get("Cargo.toml").unwrap();
        assert!(cargo_toml.contains("serde"));
        assert!(cargo_toml.contains("tokio"));
        assert!(cargo_toml.contains("async-stream"));
    }

    #[test]
    fn test_snake_case_conversion() {
        let ir = create_test_ir();
        let client_content = rust::client::generate_client(&ir);

        // CamelCase method names should be converted to snake_case
        // (Note: in this test data all methods are already snake_case,
        // but the converter should handle it correctly)
        assert!(client_content.contains("pub async fn check"));
        assert!(client_content.contains("pub async fn watch"));
    }
}
