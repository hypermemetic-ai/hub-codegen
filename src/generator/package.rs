//! Package configuration generation
//!
//! Generates package.json and tsconfig.json for the TypeScript client.

use crate::ir::IR;

/// Generate package.json content
pub fn generate_package_json(ir: &IR) -> String {
    let plexus_hash = ir.ir_hash.as_deref().unwrap_or("unknown");
    let version_hash = if plexus_hash.len() >= 16 {
        &plexus_hash[..16]
    } else {
        plexus_hash
    };

    format!(r#"{{
  "name": "@plexus/client",
  "version": "0.0.0-{version_hash}",
  "type": "module",
  "main": "index.ts",
  "scripts": {{
    "test": "npx tsx test/smoke.test.ts",
    "typecheck": "npx tsc --noEmit"
  }},
  "dependencies": {{
    "ws": "^8.0.0"
  }},
  "devDependencies": {{
    "tsx": "^4.0.0",
    "typescript": "^5.0.0",
    "@types/ws": "^8.0.0"
  }}
}}
"#)
}

/// Generate tsconfig.json content
pub fn generate_tsconfig() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "skipLibCheck": true,
    "noEmit": true,
    "types": ["node"]
  },
  "include": ["*.ts", "test/*.ts"]
}
"#.to_string()
}
