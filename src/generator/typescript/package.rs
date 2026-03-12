//! Package configuration generation
//!
//! Generates package.json and tsconfig.json for the TypeScript client.

use std::collections::HashMap;
use crate::generator::TransportEnv;

/// Generate package.json content.
///
/// `version_hash` should be a content-based hash computed from the other
/// generated files — NOT the IR hash. This ensures the package version only
/// changes when generated code changes, not when IR metadata (timestamps,
/// unrelated plugin additions) changes.
pub fn generate_package_json(transport: TransportEnv, has_bidir: bool, version_hash: &str) -> String {

    // Conditionally include deps based on transport
    let dependencies = match transport {
        TransportEnv::Ws => r#"    "ws": "^8.18.0""#,
        TransportEnv::Browser => "",  // transport.ts uses native window.WebSocket — no npm dep needed
        TransportEnv::None => r#"    "@plexus/rpc-client": "workspace:*""#,
    };

    // Include bidir test script if bidir methods exist
    let scripts = if has_bidir {
        r#""test": "bun test",
    "test:bidir": "bun test test/bidir-smoke.test.ts",
    "test:all": "bun test",
    "typecheck": "bun x tsc --noEmit""#
    } else {
        r#""test": "bun test",
    "typecheck": "bun x tsc --noEmit""#
    };

    let dev_dependencies = match transport {
        TransportEnv::Browser => r#"    "bun-types": "^1.0.0",
    "typescript": "^5.0.0",
    "@types/node": "^20.0.0""#,
        _ => r#"    "bun-types": "^1.0.0",
    "typescript": "^5.0.0",
    "@types/ws": "^8.0.0",
    "@types/node": "^20.0.0""#,
    };

    format!(r#"{{
  "name": "@plexus/client",
  "version": "0.0.0-{version_hash}",
  "type": "module",
  "main": "index.ts",
  "_generatedBy": "hub-codegen",
  "scripts": {{
    {scripts}
  }},
  "dependencies": {{
{dependencies}
  }},
  "devDependencies": {{
{dev_dependencies}
  }}
}}
"#)
}

/// Return the runtime npm dependencies for the given transport
pub fn get_runtime_deps(transport: TransportEnv) -> HashMap<String, String> {
    match transport {
        TransportEnv::Ws => [("ws".to_string(), "^8.18.0".to_string())].into_iter().collect(),
        _ => HashMap::new(),
    }
}

/// Return the npm dev dependencies for the given transport
pub fn get_dev_deps(transport: TransportEnv) -> HashMap<String, String> {
    let mut deps: HashMap<String, String> = [
        ("bun-types", "^1.0.0"),
        ("typescript", "^5.0.0"),
        ("@types/node", "^20.0.0"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    if transport == TransportEnv::Ws {
        deps.insert("@types/ws".to_string(), "^8.0.0".to_string());
    }
    deps
}

/// Generate tsconfig.json content
pub fn generate_tsconfig(transport: TransportEnv) -> String {
    // Browser mode: use DOM lib so WebSocket is a known global (no ws import).
    // Ws/None modes: use node types (ws package provides WebSocket via @types/ws).
    let type_config = match transport {
        TransportEnv::Browser => r#""lib": ["ES2022", "DOM"]"#,
        _ => r#""types": ["node"]"#,
    };
    format!(r#"{{
  "compilerOptions": {{
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "skipLibCheck": true,
    "noEmit": true,
    {type_config}
  }},
  "include": ["*.ts", "test/*.ts"]
}}
"#)
}
