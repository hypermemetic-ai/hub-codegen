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
