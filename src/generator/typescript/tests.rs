//! Smoke test generation
//!
//! Generates test/smoke.test.ts for verifying basic connectivity
//! and test/bidir-smoke.test.ts for bidirectional communication.

use crate::ir::IR;
use crate::generator::TransportEnv;

/// Check if the IR contains bidirectional methods
///
/// Looks for the "interactive" namespace which is the standard bidir demo
pub fn has_bidir_methods(ir: &IR) -> bool {
    ir.ir_plugins.contains_key("interactive")
}

/// Generate smoke test content
pub fn generate_smoke_test(ir: &IR, transport: TransportEnv) -> String {
    let backend = &ir.ir_backend;

    let import_line = if transport != TransportEnv::None {
        "import { PlexusRpcClient } from '../transport';"
    } else {
        "import { PlexusRpcClient } from '@plexus/rpc-client';"
    };

    format!(r#"// Auto-generated smoke test for {backend} backend
// Run with: bun test

import {{ test, expect, beforeAll, afterAll }} from "bun:test";
{import_line}
import type {{ PlexusStreamItem }} from "../types";

const WS_URL = process.env.PLEXUS_URL ?? "ws://localhost:4444";

let client: PlexusRpcClient;

beforeAll(async () => {{
  client = new PlexusRpcClient({{
    backend: "{backend}",
    url: WS_URL,
    debug: false,
    connectionTimeout: 5000,
  }});
  await client.connect();
}}, 10_000);

afterAll(() => {{
  client?.disconnect();
}});

test("connects to {backend} backend", () => {{
  expect(client).toBeDefined();
}});

test("{backend}.schema returns stream ending in done", async () => {{
  const items: PlexusStreamItem[] = [];
  for await (const item of client.call("{backend}.schema", {{}})) {{
    items.push(item);
    if (item.type === "done") break;
    if (item.type === "error" && !item.recoverable) {{
      throw new Error(`Backend error: ${{item.message}}`);
    }}
  }}
  expect(items.length).toBeGreaterThan(0);
  expect(items[items.length - 1].type).toBe("done");
}}, 10_000);
"#, backend = backend, import_line = import_line)
}

/// Generate schema walk smoke test (no test framework, plain executable TypeScript).
///
/// Uses well-known Plexus endpoints:
///   `_info` → `{backend}.schema` → `{backend}.activation_schema`
/// These exist on every conformant Plexus backend regardless of loaded plugins.
pub fn generate_schema_walk_smoke(_ir: &IR, transport: TransportEnv, transport_path: &str) -> String {
    let import_line = if transport != TransportEnv::None {
        format!("import {{ PlexusRpcClient }} from \"{transport_path}\";")
    } else {
        "import { PlexusRpcClient } from '@plexus/rpc-client';".to_string()
    };

    format!(r#"// Auto-generated schema walk smoke test
// Run with: bun smoke.ts  (no test framework required)

{import_line}

const URL = process.env.PLEXUS_URL ?? "ws://127.0.0.1:4444";
const rpc = new PlexusRpcClient({{ url: URL }});

function assert(cond: boolean, msg: string): asserts cond {{
  if (!cond) {{ rpc.disconnect(); throw new Error(msg); }}
}}

await rpc.connect();

// 1. _info — well-known, no namespace, proves connectivity
const info = await rpc.callOnce("_info", null);
assert(typeof info?.backend === "string", "_info must return {{ backend: string }}");
const backend = info.backend;

// 2. schema walk — discover all activations
const schema = await rpc.callOnce(`${{backend}}.schema`, []);
assert(Array.isArray(schema?.activations), `${{backend}}.schema must return activations`);
assert(schema.activations.length > 0, `${{backend}}.schema returned 0 activations`);

// 3. activation_schema per plugin — validates schema coherence
for (const act of schema.activations) {{
  const detail = await rpc.callOnce(`${{backend}}.activation_schema`, [act.namespace]);
  assert(detail != null, `activation_schema for ${{act.namespace}} must respond`);
}}

rpc.disconnect();
console.log(`\u2713 ${{schema.activations.length}} activations validated (${{backend}})`);
"#)
}

/// Generate bidirectional smoke test content
///
/// Tests the interactive.wizard method which exercises all bidir request types:
/// - prompt (text input)
/// - select (option selection)
/// - confirm (yes/no)
pub fn generate_bidir_smoke_test(ir: &IR, transport: TransportEnv) -> String {
    let backend = &ir.ir_backend;

    let import_line = if transport != TransportEnv::None {
        "import { PlexusRpcClient } from '../transport';"
    } else {
        "import { PlexusRpcClient } from '@plexus/rpc-client';"
    };

    format!(r#"// Auto-generated bidirectional smoke test for {backend} backend
// Run with: bun test

import {{ test, expect, beforeAll, afterAll }} from "bun:test";
{import_line}
import type {{ StandardRequest, StandardResponse }} from "../types";

const WS_URL = process.env.PLEXUS_URL ?? "ws://localhost:4444";

let client: PlexusRpcClient;
const requestsReceived: StandardRequest[] = [];

beforeAll(async () => {{
  client = new PlexusRpcClient({{
    backend: "{backend}",
    url: WS_URL,
    debug: false,
    connectionTimeout: 5000,
    onBidirectionalRequest: async (request: StandardRequest): Promise<StandardResponse | undefined> => {{
      requestsReceived.push(request);
      if (request.type === "prompt") {{
        return {{ type: "text", value: "test-project" }};
      }}
      if (request.type === "select") {{
        const first = (request as any).options?.[0]?.value ?? "default";
        return {{ type: "selected", values: [first] }};
      }}
      if (request.type === "confirm") {{
        return {{ type: "confirmed", value: true }};
      }}
      return {{ type: "cancelled" }};
    }},
  }});
  await client.connect();
}}, 10_000);

afterAll(() => {{
  client?.disconnect();
}});

test("connects with bidirectional handler", () => {{
  expect(client).toBeDefined();
}});

test("interactive.wizard receives all request types", async () => {{
  for await (const item of client.call("interactive.wizard", {{}})) {{
    if (item.type === "done") break;
    if (item.type === "error" && !item.recoverable) {{
      throw new Error(`Backend error: ${{item.message}}`);
    }}
  }}
  expect(requestsReceived.some(r => r.type === "prompt")).toBe(true);
  expect(requestsReceived.some(r => r.type === "select")).toBe(true);
  expect(requestsReceived.some(r => r.type === "confirm")).toBe(true);
}}, 30_000);
"#, backend = backend, import_line = import_line)
}
