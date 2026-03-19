# IR Data Model Architecture

## Overview

The **Intermediate Representation (IR)** is a language-agnostic JSON schema that bridges Rust type definitions in substrate plugins with typed client code generation. The IR is produced by Synapse's IR Builder (Haskell) via schema introspection, deserialized by hub-codegen into Rust structs, and then consumed by language-specific generators to emit TypeScript (and Rust) clients.

```
Substrate Plugins (Rust types)
  ↓ schemars JSON Schema
Synapse IR Builder (Haskell)
  ↓ IR v2.0 JSON
hub-codegen IR Deserialization (Rust)
  ↓ structured IR types
Language Generators (TypeScript / Rust)
  ↓
Generated Client Code + Dependencies
```

---

## IR JSON Schema

The top-level IR JSON object:

| Field | Type | Required | Semantics |
|-------|------|----------|-----------|
| `irVersion` | string | Yes | Must be `"2.0"`. Mismatches abort generation. |
| `irBackend` | string | Yes | Backend name (e.g. `"substrate"`). Used in file headers. |
| `irHash` | string | No | 16-char hex content hash (stable across timestamps). Used by synapse-cc for cache invalidation. |
| `irMetadata` | object | No | Toolchain metadata (generators, timestamps). Intentionally **excluded from file hashes** to prevent spurious cache churn. |
| `irTypes` | object | Yes | `"namespace.LocalName"` → `TypeDef`. Empty namespace uses `".LocalName"` (dot prefix). |
| `irMethods` | object | Yes | `"namespace.method"` → `MethodDef`. |
| `irPlugins` | object | Yes | `namespace` → `[method names]`. Every method appears in exactly one plugin entry. |

---

## Rust Type Definitions

**File**: `src/ir.rs`

### `IR` struct (`ir.rs:33`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IR {
    pub ir_version: String,
    pub ir_backend: String,
    pub ir_hash: Option<String>,
    pub ir_metadata: Option<GenerationMetadata>,
    pub ir_types: HashMap<String, TypeDef>,
    pub ir_methods: HashMap<String, MethodDef>,
    pub ir_plugins: HashMap<String, Vec<String>>,
}
```

### `TypeDef` (`ir.rs:53`)

```rust
pub struct TypeDef {
    pub td_name: String,              // "ChatEvent"
    pub td_namespace: String,         // "cone"
    pub td_description: Option<String>,
    pub td_kind: TypeKind,
}
```

`full_name()` returns `"cone.ChatEvent"` — must match the map key in `ir_types`.

### `TypeKind` — tagged union (`ir.rs:71`, uses `#[serde(tag = "tag")]`)

| Variant | Fields | TypeScript output |
|---------|--------|------------------|
| `KindStruct` | `ks_fields: Vec<FieldDef>` | `interface` |
| `KindEnum` | `ke_discriminator: String`, `ke_variants: Vec<VariantDef>` | Discriminated union + type guards |
| `KindAlias` | `ka_target: TypeRef` | `type` alias |
| `KindPrimitive` | `kp_type: String`, `kp_format: Option<String>` | `string` / `number` / etc. |
| `KindStringEnum` | `kse_values: Vec<String>` | `'a' \| 'b' \| 'c'` union |

`KindEnum.ke_discriminator` is always `"type"` by convention. Variant names match the discriminator value in the generated interfaces.

### `TypeRef` — custom deserializer (`ir.rs:164`)

The most important type. Haskell Aeson emits `{"tag": "RefFoo", "contents": ...}`:

| Tag | Rust | TypeScript |
|-----|------|-----------|
| `RefNamed` | `RefNamed(QualifiedName)` | Local type name (with cross-namespace import) |
| `RefPrimitive` | `RefPrimitive(String, Option<String>)` | `string` / `number` / `boolean` |
| `RefArray` | `RefArray(Box<TypeRef>)` | `T[]` |
| `RefOptional` | `RefOptional(Box<TypeRef>)` | `T \| null` |
| `RefAny` | `RefAny` | `unknown` (intentional, no warning) |
| `RefUnknown` | `RefUnknown` | `unknown` (schema gap, emits warning) |

`to_ts_in_namespace(current_ns)` returns just the local name when the type is in the same namespace; cross-namespace types are imported separately.

### `MethodDef` (`ir.rs:242`)

```rust
pub struct MethodDef {
    pub md_name: String,              // "chat"
    pub md_full_path: String,         // "cone.chat"  (wire method name)
    pub md_namespace: String,         // "cone"
    pub md_streaming: bool,           // true → AsyncGenerator<T>
    pub md_params: Vec<ParamDef>,
    pub md_returns: TypeRef,
    pub md_bidir_type: Option<TypeRef>, // present if bidirectional
}
```

`md_full_path` is passed verbatim to `rpc.call()` on the wire.

### `QualifiedName` (`ir.rs:131`)

```rust
pub struct QualifiedName {
    pub qn_namespace: String,   // "cone"
    pub qn_local_name: String,  // "ChatEvent"
}
```

`full_name()` → `"cone.ChatEvent"` or just `"PlexusStreamItem"` when namespace is empty.

### `FieldDef` / `ParamDef` (`ir.rs:107`, `ir.rs:276`)

Both share the same shape: `name`, `type: TypeRef`, `description?`, `required: bool`, `default?`. `fd_required = false` generates optional (`?`) TypeScript fields.

### `VariantDef` (`ir.rs:120`)

```rust
pub struct VariantDef {
    pub vd_name: String,              // "data" | "error" | etc.
    pub vd_description: Option<String>,
    pub vd_fields: Vec<FieldDef>,
}
```

The discriminator value is `vd_name`; variant interface name is `{TypeName}{PascalCase(vd_name)}`.

---

## IR Deserialization Flow

`main.rs:119–131`:

1. If `--generate transport` — inject minimal dummy IR (transport.ts needs no IR)
2. Otherwise read from `--input` file or stdin
3. `serde_json::from_str(&buf)` → `IR`

**Version check** (`typescript/mod.rs:143–150`): bail if `ir_version != "2.0"`.

---

## Concrete IR Examples

### Struct type

```json
{
  "tdName": "Handle",
  "tdNamespace": "arbor",
  "tdKind": {
    "tag": "KindStruct",
    "ksFields": [
      { "fdName": "plugin_id", "fdRequired": true,  "fdType": { "tag": "RefPrimitive", "contents": ["string", "uuid"] } },
      { "fdName": "meta",      "fdRequired": false, "fdType": { "tag": "RefArray", "contents": { "tag": "RefPrimitive", "contents": ["string", null] } } }
    ]
  }
}
```

→ TypeScript:
```typescript
export interface Handle {
  plugin_id: string;
  meta?: string[];
}
```

### Discriminated union

```json
{
  "tag": "KindEnum",
  "keDiscriminator": "type",
  "keVariants": [
    { "vdName": "data",  "vdFields": [ { "fdName": "content", "fdRequired": true, "fdType": { "tag": "RefAny" } } ] },
    { "vdName": "error", "vdFields": [ { "fdName": "message", "fdRequired": true, "fdType": { "tag": "RefPrimitive", "contents": ["string", null] } } ] }
  ]
}
```

→ TypeScript:
```typescript
export interface PlexusStreamItemData  { type: 'data';  content: unknown }
export interface PlexusStreamItemError { type: 'error'; message: string }
export type PlexusStreamItem = PlexusStreamItemData | PlexusStreamItemError;
```

### Plugin organization

```json
{
  "irPlugins": {
    "cone":  ["chat", "create", "delete", "get", "list"],
    "arbor": ["tree_create", "tree_delete", "context_get_handles"],
    "":      [".call", ".hash", ".schema"]
  }
}
```

Empty namespace `""` holds root-level methods. Every method in `irMethods` appears in exactly one plugin.

---

## Invariants

1. **Type key format** — `"namespace.LocalName"` or `".LocalName"` for empty namespace; must match `td_namespace.td_name`
2. **Method key format** — `"namespace.method"` or `".method"`; matches `md_full_path`
3. **No cycles** — `TypeRef` references must be acyclic
4. **RefNamed targets exist** — all named refs point to keys in `irTypes`
5. **Plugin completeness** — every method in `irMethods` appears in exactly one `irPlugins` entry
6. **Discriminator convention** — all `KindEnum` use `"type"` as discriminator

---

## Warning System

`RefUnknown` (schema gap, not intentional like `RefAny`) triggers a `Warning`:

```rust
pub struct Warning { pub location: String, pub message: String }
```

Warnings are printed to stderr and included in the JSON output (`CodegenOutput.warnings`). They do not abort generation — the field becomes `unknown` in the output.

---

## Related Documents

- `docs/architecture/16679174944041108735_method-schema-spec.md` — Method schema patterns (source of IR)
- `docs/architecture/16679314737030628607_ir-codegen-chain.md` — Full IR → codegen pipeline
- `docs/architecture/cli-and-pipeline.md` — How IR is consumed by the CLI
- `docs/architecture/typescript-generator.md` — How IR maps to TypeScript artifacts
