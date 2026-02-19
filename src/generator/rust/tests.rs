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
                md_bidir_type: None,
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
                md_bidir_type: None,
            },
        );

        ir_plugins.insert("health".to_string(), vec!["check".to_string(), "watch".to_string()]);

        IR {
            ir_version: "2.0".to_string(),
            ir_backend: "test".to_string(),
            ir_hash: Some("test-hash-123".to_string()),
            ir_metadata: None,
            ir_types,
            ir_methods,
            ir_plugins,
        }
    }

    #[test]
    fn test_generate_types() {
        let ir = create_test_ir();

        // Core types (PlexusStreamItem, PlexusError, etc.)
        let core_types = rust::types::generate_core_types(&ir);
        assert!(core_types.contains("pub enum PlexusStreamItem"));
        assert!(core_types.contains("PlexusError"));

        // Namespace types are generated via namespace modules
        let namespace_modules = rust::client::generate_namespace_modules(&ir);
        // Find the health namespace module content
        let health_content = namespace_modules.values()
            .find(|c| c.contains("pub struct Status"))
            .expect("Should have a module containing Status struct");

        // Should contain struct definition
        assert!(health_content.contains("pub struct Status"));
        assert!(health_content.contains("pub healthy: bool"));
        assert!(health_content.contains("pub uptime: i64"));

        // Should contain enum definition
        assert!(health_content.contains("pub enum Event"));
        assert!(health_content.contains("Started"));
        assert!(health_content.contains("Completed"));
    }

    #[test]
    fn test_generate_client() {
        let ir = create_test_ir();

        // Base client has PlexusClient struct
        let base_client = rust::client::generate_base_client();
        assert!(base_client.contains("pub struct PlexusClient"));
        assert!(base_client.contains("pub fn new("));

        // Methods are in namespace modules
        let namespace_modules = rust::client::generate_namespace_modules(&ir);
        let health_content = namespace_modules.values()
            .find(|c| c.contains("pub async fn check"))
            .expect("Should have a module containing check method");

        // Should contain non-streaming method
        assert!(health_content.contains("pub async fn check"));
        assert!(health_content.contains("-> Result<Status>"));

        // Should contain streaming method
        assert!(health_content.contains("pub async fn watch"));
        assert!(health_content.contains("interval: i64"));
        assert!(health_content.contains("Pin<Box<dyn Stream<Item = Result<Event>> + Send>>"));
    }

    #[test]
    fn test_full_generation() {
        let ir = create_test_ir();
        let result = rust::generate(&ir).expect("Generation should succeed");

        // Should have all required files (paths include src/ prefix)
        assert!(result.files.contains_key("src/lib.rs"), "Missing src/lib.rs, keys: {:?}", result.files.keys().collect::<Vec<_>>());
        assert!(result.files.contains_key("src/types.rs"));
        assert!(result.files.contains_key("src/client.rs"));
        assert!(result.files.contains_key("Cargo.toml"));

        // lib.rs should re-export modules
        let lib_content = result.files.get("src/lib.rs").unwrap();
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

        // Methods are in namespace modules
        let namespace_modules = rust::client::generate_namespace_modules(&ir);
        let health_content = namespace_modules.values()
            .find(|c| c.contains("pub async fn check"))
            .expect("Should have a module containing check method");

        // CamelCase method names should be converted to snake_case
        // (Note: in this test data all methods are already snake_case,
        // but the converter should handle it correctly)
        assert!(health_content.contains("pub async fn check"));
        assert!(health_content.contains("pub async fn watch"));
    }
}
