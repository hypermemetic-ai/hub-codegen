# TypeScript Generator Architecture

## Overview

Hub-codegen's TypeScript generator transforms Synapse IR into a complete, type-safe TypeScript client library for plexus backends. Generation produces two distinct layers:

- **Layer 1 (RPC)**: Raw async generator interface over a WebSocket transport
- **Layer 2 (Typed)**: Domain-specific client wrappers with automatic `PlexusStreamItem` unwrapping

---

## Generated File Set

When `--generate all` (default), the following files are produced:

| File | Purpose | Static/Dynamic |
|------|---------|----------------|
| `types.ts` | Core protocol types (PlexusStreamItem, guards, bidirectional types) | Static |
| `rpc.ts` | RpcClient interface + helpers (`extractData`, `collectOne`) | Static |
| `transport.ts` | WebSocket/browser transport implementation | Static (variant per `--transport`) |
| `tsconfig.json` | TypeScript compiler config | Static (variant per `--transport`) |
| `package.json` | Package metadata + dependencies | Dynamic (version hash from code) |
| `{ns}/types.ts` | Domain type definitions per namespace | Dynamic |
| `{ns}/client.ts` | Typed RPC client per namespace | Dynamic |
| `{ns}/index.ts` | Namespace barrel re-export | Dynamic |
| `index.ts` | Root barrel re-export | Dynamic |
| `test/smoke.test.ts` | Connectivity smoke test (bun:test) | Dynamic |
| `test/bidir-smoke.test.ts` | Bidirectional smoke test (if bidir methods exist) | Dynamic |
| `.codegen-metadata.json` | Toolchain versions + file hashes | Dynamic |

Nested namespaces follow directory convention: `solar.earth.luna` → `solar/earth/luna/{types,client,index}.ts`.

---

## File Details

### `types.ts` — Layer 1: Protocol Types

**Source**: `src/generator/typescript/types.rs:298`

Entirely static (identical across all IR). Contains:

- **`PlexusStreamItem`** discriminated union: `data | progress | error | done | request`
- Type guards (`isPlexusStreamItemData`, etc.)
- **`StandardRequest`** union: `confirm | prompt | select` (server→client bidirectional requests)
- **`StandardResponse`** union: `confirmed | text | selected | cancelled`
- **`PlexusResponse`**: wraps a response with `requestId` for correlation
- **`PlexusError`** class: exception with `code`, `recoverable`, `metadata`

### `rpc.ts` — Layer 1: RPC Helpers

**Source**: `src/generator/typescript/rpc.rs:7`

Entirely static. Contains:

- **`RpcClient` interface**: single method `call(method, params): AsyncGenerator<PlexusStreamItem>`
- **`toCamelCase()`** / **`transformKeys()`**: automatic `snake_case` → `camelCase` key transformation on responses
- **`extractData<T>(stream)`**: yields unwrapped `T` from data items, throws on error
- **`collectOne<T>(stream)`**: awaits single data item or throws

### `transport.ts` — Layer 1: WebSocket Transport

**Source**: `src/generator/typescript/transport.rs:9`

Static template with one conditional line: the `import WebSocket from 'ws'` is stripped for browser mode.

Contents:
- **`PlexusRpcConfig`**: `backend`, `url`, `connectionTimeout`, `debug`, `onBidirectionalRequest`
- **`PlexusRpcClient`** class: implements `RpcClient`, manages JSON-RPC 2.0 subscriptions, handles bidirectional requests, connection lifecycle
- **`createClient(config)`** factory

### `{namespace}/types.ts` — Layer 2: Domain Types

**Source**: `src/generator/typescript/types.rs:485`

Dynamic per namespace. Contains:

- Cross-namespace imports (relative paths computed via `calculate_relative_import_path()`)
- Per `TypeDef` in the namespace:
  - **Struct** → TypeScript `interface`
  - **Discriminated union** → variant interfaces + union type + type guards
  - **Alias** → `type` alias
  - **Primitive** → format-aware alias (e.g., uuid → `string`)
  - **StringEnum** → union of string literals
- Stub types (`unknown`) for referenced-but-undefined types

**Naming**: type names in `PascalCase`, fields in `camelCase`.

**Discriminated union pattern** (`ke_discriminator` field, typically `"type"`):
```typescript
export interface EchoEventEcho { type: 'echo'; message: string }
export interface EchoEventPong { type: 'pong' }
export type EchoEvent = EchoEventEcho | EchoEventPong;
export function isEchoEventEcho(e: EchoEvent): e is EchoEventEcho { return e.type === 'echo' }
```

### `{namespace}/client.ts` — Layer 2: Typed Client

**Source**: `src/generator/typescript/namespaces.rs:14`

Dynamic per namespace. Contains:

- **`{Namespace}Client` interface**: streaming methods return `AsyncGenerator<T>`, non-streaming return `Promise<T>`
- **`{Namespace}ClientImpl` class**: implements the interface, wraps `rpc.call()` with `extractData`/`collectOne`
- **`create{Namespace}Client(rpc)` factory**

Parameter wire format: `snake_case` preserved in the params object (`{ user_id: userId }`), while TypeScript method signatures use `camelCase` locals. Only helpers actually used by the namespace's methods are imported (`extractData`, `collectOne`, or both).

---

## Transport Variants

| Aspect | `ws` (default) | `browser` | `none` |
|--------|---------------|-----------|--------|
| `transport.ts` emitted | Yes, with `import WebSocket from 'ws'` | Yes, without ws import | **Not emitted** |
| Runtime dep | `ws: ^8.18.0` | None | None |
| Dev dep | `@types/ws: ^8.0.0` | None | None |
| tsconfig | `"types": ["node"]` | `"lib": ["ES2022", "DOM"]` | `"types": ["node"]` |
| Smoke test import | `from '../transport'` | `from '../transport'` | `from '@plexus/rpc-client'` |
| Use case | Node.js / test runners | Browser / Tauri / WebView | Monorepo (external transport) |

**None mode** adds `@plexus/rpc-client: workspace:*` as a runtime dep (monorepo convention).

---

## Generation Selectors (`--generate`)

| Selector | Artifacts | IR required? |
|----------|-----------|-------------|
| `all` | Everything | Yes |
| `transport` | `types.ts`, `rpc.ts`, `transport.ts` only | No (static template) |
| `rpc` | `types.ts`, `rpc.ts`, `index.ts` | Yes |
| `plugins` | Namespace `{types,client,index}.ts` only | Yes |
| `smoke` | `smoke.ts` walk test (no test framework) | Yes |
| `package` | `package.json` only | Yes (for version hash) |

---

## IR → TypeScript Mapping

### TypeRef Resolution

| TypeRef | TypeScript |
|---------|-----------|
| `RefNamed(ns, name)` | `PascalCase(name)` (local) or imported from other namespace |
| `RefPrimitive("string", Some("uuid"))` | `string` |
| `RefPrimitive("integer", _)` | `number` |
| `RefArray(inner)` | `T[]` |
| `RefOptional(inner)` | `T \| null` (not `undefined`) |
| `RefAny` | `unknown` (no warning — intentional) |
| `RefUnknown` | `unknown` (warning emitted — schema gap) |

### Method → Client Method

- `md_streaming = true` → `async *method(...): AsyncGenerator<T>` with `yield* extractData(...)`
- `md_streaming = false` → `async method(...): Promise<T>` with `return collectOne(...)`
- `md_full_path` is the wire method name passed to `rpc.call()`

---

## Plugin Filtering (`--plugins`)

When `--plugins echo,solar` is specified with `--generate all`:

1. **Walk types**: collect all `RefNamed` cross-namespace references from requested plugins
2. **Resolve transitively**: add referenced namespaces to `type_deps`; repeat until fixed point
3. **Partition**:
   - `requested` → full generation (client + types + index)
   - `type_deps` → `types.ts` only (stubs)
   - others → not generated

**Filter matching** (`types.rs:515–520`):
```rust
filter.iter().any(|f| f == namespace || namespace.starts_with(&format!("{f}.")))
```
- `"solar"` matches `solar`, `solar.earth`, `solar.earth.luna`
- Dot-segment aware: `"sol"` does NOT match `solar`

---

## Package.json Version Stability

**Two-pass generation** (`mod.rs:230–239`):

1. Generate all code files (excluding `package.json`)
2. `compute_plugin_hash(&files)` → 16-char version hash
3. Embed in `package.json` version: `"0.0.0-{hash}"`

Version only changes when generated code changes — not on IR timestamp/metadata churn.

---

## Cross-Namespace Import Paths

**Function**: `calculate_relative_import_path(from_namespace, to_namespace)` (`types.rs:311`)

```rust
let from_depth = from_namespace.matches('.').count();
let ups = "../".repeat(from_depth + 1);  // +1: we're inside types.ts
let to_path = to_namespace.replace('.', "/");
format!("{}{}", ups, to_path)
```

Example: from `solar/earth/luna/types.ts` (depth=2) to `io/types.ts`:
→ `"../".repeat(3)` + `"io"` = `"../../../io"` → `import type { IoType } from '../../../io/types'`

---

## Naming Conventions

- Type names: `PascalCase` (e.g., `EchoEvent`)
- Field/method names: `camelCase` (e.g., `contentType`)
- Namespace barrel exports: `SolarEarthLuna` (dots → PascalCase via `to_pascal()`)
- Variant names: `{TypeName}{VariantName}` (e.g., `EchoEventData`)

---

## Dependencies Emitted

**Runtime** (`package.rs:64–85`):
- `ws: ^8.18.0` — ws mode only

**Dev** (all modes):
- `bun-types: ^1.0.0`
- `typescript: ^5.0.0`
- `@types/node: ^20.0.0`
- `@types/ws: ^8.0.0` — ws and none modes

---

## Known Limitations

- `RefOptional` maps to `T | null` (not `T | undefined`) — matches Rust `None` → JSON `null`
- Discriminator field is always `"type"` (convention from IR builder)
- No generic type parameters in IR — each instantiation is explicit
- No schema drift detection at runtime (smoke tests provide connectivity validation)
