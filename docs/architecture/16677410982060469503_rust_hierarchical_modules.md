# Rust Generator: Hierarchical Module Structure

**Status:** Proposed (Not Yet Implemented)
**Date:** 2025-01-25
**Author:** Claude Sonnet 4.5
**Priority:** Critical - Current flat structure is incorrect

## Problem Statement

The current Rust code generator produces a **flat module structure** with underscored filenames, which does not reflect the actual namespace hierarchy. This is inconsistent with the TypeScript generator and creates poor ergonomics.

### Current State (Incorrect)

**Generated file structure:**
```
src/
├── lib.rs
├── types.rs
├── client.rs
├── cone.rs
├── arbor.rs
├── hyperforge_org.rs
├── hyperforge_org_hypermemetic.rs
├── hyperforge_org_hypermemetic_repos.rs
├── hyperforge_org_hypermemetic_secrets.rs
├── hyperforge_forge_github.rs
├── solar_earth.rs
├── solar_earth_luna.rs
├── solar_mars_phobos.rs
└── ... (50+ flat files)
```

**Usage (ugly):**
```rust
use plexus_client::hyperforge_org_hypermemetic_repos;

hyperforge_org_hypermemetic_repos::list(&client).await?;
```

**Problems:**
1. ❌ Does not reflect namespace hierarchy
2. ❌ Long, awkward module names (`hyperforge_org_hypermemetic_repos`)
3. ❌ Inconsistent with TypeScript generator
4. ❌ No logical grouping
5. ❌ Difficult to navigate 50+ files in one directory
6. ❌ Violates Rust conventions (nested modules for namespaces)

### Target State (Correct)

**Generated file structure:**
```
src/
├── lib.rs
├── types.rs              # Core PlexusStreamItem only
├── client.rs             # Base PlexusClient
├── cone/
│   └── mod.rs           # Types + methods for "cone" namespace
├── arbor/
│   └── mod.rs
├── hyperforge/
│   ├── mod.rs           # Types + methods for "hyperforge" + submodule declarations
│   ├── org/
│   │   ├── mod.rs       # Types + methods for "hyperforge.org"
│   │   ├── hypermemetic/
│   │   │   ├── mod.rs
│   │   │   ├── repos/
│   │   │   │   └── mod.rs
│   │   │   └── secrets/
│   │   │       └── mod.rs
│   │   └── juggernautlabs/
│   │       ├── mod.rs
│   │       ├── repos/
│   │       │   └── mod.rs
│   │       └── secrets/
│   │           └── mod.rs
│   ├── forge/
│   │   ├── mod.rs
│   │   ├── github/
│   │   │   └── mod.rs
│   │   └── codeberg/
│   │       └── mod.rs
│   └── workspace/
│       └── mod.rs
└── solar/
    ├── mod.rs
    ├── earth/
    │   ├── mod.rs
    │   └── luna/
    │       └── mod.rs
    ├── mars/
    │   ├── mod.rs
    │   ├── phobos/
    │   │   └── mod.rs
    │   └── deimos/
    │       └── mod.rs
    ├── jupiter/
    │   ├── mod.rs
    │   ├── io/
    │   │   └── mod.rs
    │   └── europa/
    │       └── mod.rs
    └── ...
```

**Usage (clean):**
```rust
use plexus_client::hyperforge::org::hypermemetic::repos;

// Call methods with clean path
repos::list(&client).await?;

// Or import the function directly
use plexus_client::hyperforge::org::hypermemetic::repos::list;
list(&client).await?;
```

**Benefits:**
1. ✅ Reflects actual namespace hierarchy
2. ✅ Clean, idiomatic Rust module paths
3. ✅ Consistent with TypeScript generator
4. ✅ Logical grouping by namespace
5. ✅ Easy navigation with IDE tree view
6. ✅ Follows Rust conventions

## TypeScript Generator (Reference Implementation)

The TypeScript generator already implements hierarchical structure correctly.

**Example: `hyperforge.org.hypermemetic.repos` namespace**

```
client/hyperforge/org/hypermemetic/repos/
├── client.ts        # Methods for this namespace
├── types.ts         # Types for this namespace
└── index.ts         # Re-exports
```

**Generated TypeScript usage:**
```typescript
import { Hyperforge } from '@plexus/client';

const hyperforge = new Hyperforge.Org.Hypermemetic.Repos.ReposClientImpl(rpc);
await hyperforge.list();
```

**Key insight:** Directory structure mirrors namespace structure exactly.

## Implementation Plan

### Phase 1: Parse Namespace Hierarchy

**Current:**
```rust
// Treats "hyperforge.org.hypermemetic" as flat string
let module_name = namespace.replace('.', "_");
files.insert(format!("src/{}.rs", module_name), content);
```

**New:**
```rust
/// Parse namespace into hierarchy
/// "hyperforge.org.hypermemetic" -> ["hyperforge", "org", "hypermemetic"]
fn parse_namespace_path(namespace: &str) -> Vec<String> {
    if namespace.is_empty() {
        vec![]
    } else {
        namespace.split('.').map(|s| s.to_string()).collect()
    }
}

/// Convert namespace path to file path
/// ["hyperforge", "org", "hypermemetic"] -> "src/hyperforge/org/hypermemetic/mod.rs"
fn namespace_to_file_path(path: &[String]) -> String {
    if path.is_empty() {
        "src/mod.rs".to_string()
    } else {
        format!("src/{}/mod.rs", path.join("/"))
    }
}
```

### Phase 2: Build Namespace Tree

**Data structure to represent hierarchy:**

```rust
struct NamespaceNode {
    /// Name of this namespace segment (e.g., "org")
    name: String,

    /// Full dotted path (e.g., "hyperforge.org")
    full_path: String,

    /// Methods defined at this level
    methods: Vec<MethodDef>,

    /// Types defined at this level
    types: Vec<TypeDef>,

    /// Child namespaces
    children: HashMap<String, NamespaceNode>,
}

impl NamespaceNode {
    fn new(name: String, full_path: String) -> Self {
        Self {
            name,
            full_path,
            methods: Vec::new(),
            types: Vec::new(),
            children: HashMap::new(),
        }
    }

    fn insert_method(&mut self, namespace: &str, method: MethodDef) {
        let parts = parse_namespace_path(namespace);
        self.insert_at_path(&parts, method);
    }

    fn insert_at_path(&mut self, path: &[String], method: MethodDef) {
        if path.is_empty() {
            self.methods.push(method);
        } else {
            let child = self.children
                .entry(path[0].clone())
                .or_insert_with(|| {
                    let child_path = if self.full_path.is_empty() {
                        path[0].clone()
                    } else {
                        format!("{}.{}", self.full_path, path[0])
                    };
                    NamespaceNode::new(path[0].clone(), child_path)
                });
            child.insert_at_path(&path[1..], method);
        }
    }
}
```

**Build tree from IR:**

```rust
fn build_namespace_tree(ir: &IR) -> NamespaceNode {
    let mut root = NamespaceNode::new(String::new(), String::new());

    // Insert methods
    for method in ir.ir_methods.values() {
        root.insert_method(&method.md_namespace, method.clone());
    }

    // Insert types
    for typedef in ir.ir_types.values() {
        root.insert_type(&typedef.td_namespace, typedef.clone());
    }

    root
}
```

### Phase 3: Generate Hierarchical Files

**Generate `mod.rs` for each namespace:**

```rust
fn generate_namespace_module(
    node: &NamespaceNode,
    ir: &IR,
    files: &mut HashMap<String, String>,
) {
    let mut content = vec![
        format!("//! Module for {} namespace", node.full_path),
        "//! Do not edit manually".to_string(),
        "".to_string(),
        "use crate::client::PlexusClient;".to_string(),
        "use crate::types::*;".to_string(),
        "use anyhow::{anyhow, Result};".to_string(),
        "use futures::stream::{Stream, StreamExt};".to_string(),
        "use serde::{Deserialize, Serialize};".to_string(),
        "use serde_json::json;".to_string(),
        "use std::pin::Pin;".to_string(),
        "".to_string(),
    ];

    // Declare child modules
    if !node.children.is_empty() {
        content.push("// Child namespaces".to_string());
        for child_name in node.children.keys() {
            content.push(format!("pub mod {};", child_name));
        }
        content.push("".to_string());
    }

    // Generate cross-namespace imports
    let imports = collect_cross_namespace_imports(node, ir);
    for (ns_path, types) in imports {
        for type_name in types {
            content.push(format!("use crate::{}::{};", ns_path.replace('.', "::"), type_name));
        }
    }
    content.push("".to_string());

    // Generate types for this namespace
    if !node.types.is_empty() {
        content.push("// === Types ===".to_string());
        content.push("".to_string());
        for typedef in &node.types {
            content.push(generate_typedef(typedef));
            content.push("".to_string());
        }
    }

    // Generate methods for this namespace
    if !node.methods.is_empty() {
        content.push("// === Methods ===".to_string());
        content.push("".to_string());
        for method in &node.methods {
            content.push(generate_method(method, ir, &node.full_path));
            content.push("".to_string());
        }
    }

    // Write this module
    let file_path = namespace_to_file_path(&parse_namespace_path(&node.full_path));
    files.insert(file_path, content.join("\n"));

    // Recursively generate children
    for child in node.children.values() {
        generate_namespace_module(child, ir, files);
    }
}
```

### Phase 4: Update `lib.rs` Module Declarations

**Current (flat):**
```rust
pub mod cone;
pub mod arbor;
pub mod hyperforge_org;
pub mod hyperforge_org_hypermemetic;
// ... 50+ declarations
```

**New (hierarchical):**
```rust
pub mod cone;
pub mod arbor;
pub mod hyperforge;  // Contains hyperforge::org, hyperforge::forge, etc.
pub mod solar;       // Contains solar::earth, solar::mars, etc.
// ... ~10 top-level declarations
```

**Implementation:**
```rust
fn generate_lib(root: &NamespaceNode) -> String {
    let mut lines = vec![
        "//! Auto-generated Plexus client".to_string(),
        "//! Do not edit manually".to_string(),
        "".to_string(),
        "pub mod types;".to_string(),
        "pub mod client;".to_string(),
        "".to_string(),
    ];

    // Collect top-level namespaces only
    let mut top_level: Vec<_> = root.children.keys().collect();
    top_level.sort();

    lines.push("// Top-level namespace modules".to_string());
    for name in top_level {
        lines.push(format!("pub mod {};", name));
    }

    lines.push("".to_string());
    lines.push("pub use client::PlexusClient;".to_string());
    lines.push("pub use types::PlexusStreamItem;".to_string());

    lines.join("\n")
}
```

## Cross-Namespace Imports

### Challenge

Types from one namespace used in another must be imported correctly.

**Example:** `arbor` uses `cone::UUID`

**Current (flat):**
```rust
// In arbor.rs
use crate::cone::UUID;
```

**New (hierarchical):**
```rust
// In arbor/mod.rs
use crate::cone::UUID;  // Still works! Path is crate::cone::UUID
```

**Nested example:** `hyperforge.org.hypermemetic.repos` uses `hyperforge.org.Org`

```rust
// In hyperforge/org/hypermemetic/repos/mod.rs
use crate::hyperforge::org::Org;
```

### Implementation

```rust
fn collect_cross_namespace_imports(
    node: &NamespaceNode,
    ir: &IR,
) -> HashMap<String, Vec<String>> {
    let mut imports = HashMap::new();

    // Scan methods and types for cross-namespace references
    for method in &node.methods {
        scan_type_ref(&method.md_returns, &node.full_path, ir, &mut imports);
        for param in &method.md_params {
            scan_type_ref(&param.pd_type, &node.full_path, ir, &mut imports);
        }
    }

    for typedef in &node.types {
        // Scan struct fields, enum variants, etc.
    }

    imports
}

fn scan_type_ref(
    tr: &TypeRef,
    current_namespace: &str,
    ir: &IR,
    imports: &mut HashMap<String, Vec<String>>,
) {
    match tr {
        TypeRef::RefNamed(qn) => {
            if let Some(typedef) = ir.ir_types.get(&qn.full_name()) {
                if !typedef.td_namespace.is_empty()
                    && typedef.td_namespace != current_namespace
                {
                    // Convert namespace to module path
                    // "hyperforge.org" -> "hyperforge::org"
                    let module_path = typedef.td_namespace.replace('.', "::");
                    imports
                        .entry(module_path)
                        .or_default()
                        .push(to_pascal(&qn.local_name()));
                }
            }
        }
        TypeRef::RefArray(inner) => scan_type_ref(inner, current_namespace, ir, imports),
        TypeRef::RefOptional(inner) => scan_type_ref(inner, current_namespace, ir, imports),
        _ => {}
    }
}
```

## Usage Examples

### Before (Flat - Current)

```rust
use plexus_client::PlexusClient;
use plexus_client::hyperforge_org_hypermemetic_repos;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = PlexusClient::new("ws://localhost:4444");

    // Awkward long module names
    let repos = hyperforge_org_hypermemetic_repos::list(&client).await?;

    Ok(())
}
```

### After (Hierarchical - Proposed)

```rust
use plexus_client::PlexusClient;
use plexus_client::hyperforge::org::hypermemetic::repos;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = PlexusClient::new("ws://localhost:4444");

    // Clean, hierarchical paths
    let repo_list = repos::list(&client).await?;

    // Can also import functions directly
    use plexus_client::hyperforge::org::info;
    let org_info = info(&client).await?;

    // Or use full paths
    plexus_client::solar::earth::luna::observe(&client).await?;

    Ok(())
}
```

## Migration Impact

### Generated Code Changes

**Files affected:** All namespace modules

**Breaking change:** Yes - import paths will change

**Before:**
```rust
use plexus_client::hyperforge_org;
hyperforge_org::info(&client).await?;
```

**After:**
```rust
use plexus_client::hyperforge::org;
hyperforge::org::info(&client).await?;
```

### Migration Guide for Users

1. **Update import paths:**
   ```rust
   // Old
   use plexus_client::solar_earth_luna;

   // New
   use plexus_client::solar::earth::luna;
   ```

2. **Update usage:**
   ```rust
   // Old
   solar_earth_luna::observe(&client).await?;

   // New
   luna::observe(&client).await?;
   ```

3. **Batch migration with regex:**
   ```bash
   # Replace underscore paths with :: paths
   sed -i 's/plexus_client::\([a-z_]*\)_\([a-z_]*\)/plexus_client::\1::\2/g' src/**/*.rs
   ```

### Backwards Compatibility

**Not possible** - this is a fundamental structural change.

**Recommendation:**
- Bump major version (0.1.0 → 0.2.0)
- Add migration section to CHANGELOG
- Update all examples and documentation

## Implementation Checklist

- [ ] Implement `parse_namespace_path()`
- [ ] Implement `NamespaceNode` tree structure
- [ ] Implement `build_namespace_tree()`
- [ ] Update `generate_namespace_module()` to use tree
- [ ] Update `generate_lib()` to declare top-level modules only
- [ ] Fix cross-namespace import path generation (`.` → `::`)
- [ ] Update tests to expect hierarchical structure
- [ ] Update smoke test to verify directory structure
- [ ] Regenerate from real IR and verify compilation
- [ ] Update all example programs
- [ ] Update documentation (architecture docs, README)
- [ ] Add migration guide

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_parse_namespace_path() {
    assert_eq!(
        parse_namespace_path("hyperforge.org.hypermemetic"),
        vec!["hyperforge", "org", "hypermemetic"]
    );

    assert_eq!(
        parse_namespace_path("cone"),
        vec!["cone"]
    );

    assert_eq!(
        parse_namespace_path(""),
        vec![]
    );
}

#[test]
fn test_namespace_to_file_path() {
    assert_eq!(
        namespace_to_file_path(&["hyperforge", "org", "hypermemetic"]),
        "src/hyperforge/org/hypermemetic/mod.rs"
    );

    assert_eq!(
        namespace_to_file_path(&["cone"]),
        "src/cone/mod.rs"
    );
}

#[test]
fn test_namespace_tree_build() {
    let ir = create_test_ir_with_nested_namespaces();
    let tree = build_namespace_tree(&ir);

    // Verify hyperforge.org.hypermemetic exists
    assert!(tree.children.contains_key("hyperforge"));
    let hyperforge = &tree.children["hyperforge"];
    assert!(hyperforge.children.contains_key("org"));
    let org = &hyperforge.children["org"];
    assert!(org.children.contains_key("hypermemetic"));
}
```

### Smoke Test

```rust
#[test]
fn test_hierarchical_structure() {
    let ir = create_real_ir();
    let result = generate_rust(&ir).unwrap();

    // Should have hierarchical paths
    assert!(result.files.contains_key("src/hyperforge/mod.rs"));
    assert!(result.files.contains_key("src/hyperforge/org/mod.rs"));
    assert!(result.files.contains_key("src/hyperforge/org/hypermemetic/mod.rs"));
    assert!(result.files.contains_key("src/solar/earth/luna/mod.rs"));

    // Should NOT have flat paths
    assert!(!result.files.contains_key("src/hyperforge_org.rs"));
    assert!(!result.files.contains_key("src/solar_earth_luna.rs"));
}
```

## Timeline

**Estimated effort:** 2-3 days

1. **Day 1:** Implement namespace tree and hierarchical file generation
2. **Day 2:** Fix cross-namespace imports, update tests, regenerate and verify
3. **Day 3:** Update examples, documentation, create migration guide

## References

- TypeScript generator: `src/generator/typescript/namespaces.rs`
- Current Rust generator: `src/generator/rust/client.rs`
- Generated TypeScript client: `/Users/user/dev/controlflow/hypermemetic/substrate-sandbox-ts/node_modules/@plexus/client/`
- Current generated Rust client: `/Users/user/dev/controlflow/hypermemetic/codegen/rust/plexus/client/`

## Decision

**Status:** Approved pending implementation

**Rationale:**
1. Consistency with TypeScript generator
2. Better user experience (clean import paths)
3. Idiomatic Rust (nested modules for namespaces)
4. Easier navigation (directory tree matches namespace tree)
5. Scalability (works better with 100+ namespaces)

**Next steps:** Implement changes in `hub-codegen/src/generator/rust/`
