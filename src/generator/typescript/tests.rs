//! Smoke test generation
//!
//! Generates test/smoke.test.ts for verifying basic connectivity
//! and test/bidir-smoke.test.ts for bidirectional communication.

use crate::ir::IR;

/// Check if the IR contains bidirectional methods
///
/// Looks for the "interactive" namespace which is the standard bidir demo
pub fn has_bidir_methods(ir: &IR) -> bool {
    ir.ir_plugins.contains_key("interactive")
}

/// Generate smoke test content
pub fn generate_smoke_test(ir: &IR, bundle_transport: bool) -> String {
    let backend = &ir.ir_backend;

    let import_line = if bundle_transport {
        "import { PlexusRpcClient } from '../transport';"
    } else {
        "import { PlexusRpcClient } from '@plexus/rpc-client';"
    };

    format!(r#"// Auto-generated smoke test for {backend} backend
// Run with: npm test
//
// This test connects to the running {backend} server and verifies basic connectivity.
// It will skip gracefully if the server is not running.

{import_line}

async function smokeTest() {{
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('  Smoke Test: {backend} Backend Connection');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  console.log('Creating PlexusRpcClient...');
  const client = new PlexusRpcClient({{
    backend: '{backend}',
    url: 'ws://localhost:4444',
    debug: false,
    connectionTimeout: 5000
  }});

  let connected = false;

  try {{
    console.log('Connecting to ws://localhost:4444...');
    await client.connect();
    connected = true;
    console.log('✅ Connected successfully\n');

    console.log('Calling {backend}.schema to verify backend is responding...');
    const items: any[] = [];

    for await (const item of client.call('{backend}.schema', {{}})) {{
      items.push(item);
      if (item.type === 'done') {{
        console.log(`✅ Received response with ${{items.length}} stream items`);
        break;
      }}
      if (item.type === 'error') {{
        throw new Error(`Backend returned error: ${{item.message}}`);
      }}
    }}

    if (items.length === 0) {{
      throw new Error('No items received from {backend}.schema call');
    }}

    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('  ✅ Smoke test PASSED');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  }} catch (err: any) {{
    if (!connected && (err.code === 'ECONNREFUSED' || err.message?.includes('connect'))) {{
      console.log('\n⚠️  Server not running at ws://localhost:4444');
      console.log('   This is expected if the server is not started.');
      console.log('   Skipping smoke test.\n');
      process.exit(0); // Exit successfully (don\'t fail CI)
    }}

    console.error('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.error('  ❌ Smoke test FAILED');
    console.error('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.error('\nError:', err.message || err);
    if (err.stack) {{
      console.error('\nStack trace:');
      console.error(err.stack);
    }}
    console.error('');
    process.exit(1);

  }} finally {{
    if (connected) {{
      client.disconnect();
      console.log('Disconnected from server.\n');
    }}
  }}
}}

// Run the test
smokeTest();
"#, backend = backend, import_line = import_line)
}

/// Generate bidirectional smoke test content
///
/// Tests the interactive.wizard method which exercises all bidir request types:
/// - prompt (text input)
/// - select (option selection)
/// - confirm (yes/no)
pub fn generate_bidir_smoke_test(ir: &IR, bundle_transport: bool) -> String {
    let backend = &ir.ir_backend;

    let import_line = if bundle_transport {
        "import { PlexusRpcClient } from '../transport';"
    } else {
        "import { PlexusRpcClient } from '@plexus/rpc-client';"
    };

    format!(r#"// Auto-generated bidirectional smoke test for {backend} backend
// Run with: npm run test:bidir
//
// This test connects to the running {backend} server and tests bidirectional
// communication via the interactive.wizard method. It will skip gracefully
// if the server is not running.

{import_line}
import type {{ StandardRequest, StandardResponse }} from '../types';

const PORT = process.env.{backend_upper}_PORT || '4444';

async function bidirSmokeTest() {{
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  console.log('  Bidirectional Smoke Test: interactive.wizard');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  // Track bidirectional requests received
  const requestsReceived: StandardRequest[] = [];

  console.log('Creating PlexusRpcClient with bidirectional handler...');
  const client = new PlexusRpcClient({{
    backend: '{backend}',
    url: `ws://localhost:${{PORT}}`,
    debug: false,
    connectionTimeout: 5000,
    onBidirectionalRequest: async (request: StandardRequest): Promise<StandardResponse | undefined> => {{
      console.log(`  📥 Received ${{request.type}} request`);
      requestsReceived.push(request);

      // Respond based on request type
      if (request.type === 'prompt') {{
        return {{ type: 'text', value: 'test-project' }};
      }}
      if (request.type === 'select') {{
        const firstOption = request.options?.[0]?.value || 'default';
        return {{ type: 'selected', values: [firstOption] }};
      }}
      if (request.type === 'confirm') {{
        return {{ type: 'confirmed', value: true }};
      }}

      return {{ type: 'cancelled' }};
    }}
  }});

  let connected = false;

  try {{
    console.log(`Connecting to ws://localhost:${{PORT}}...`);
    await client.connect();
    connected = true;
    console.log('✅ Connected successfully\n');

    console.log('Calling interactive.wizard...');
    const events: any[] = [];

    for await (const item of client.call('interactive.wizard', {{}})) {{
      events.push(item);

      if (item.type === 'done') {{
        break;
      }}
      if (item.type === 'error' && !item.recoverable) {{
        throw new Error(`Backend returned error: ${{item.message}}`);
      }}
    }}

    console.log('');

    // Verify we got expected bidirectional requests
    const hasPrompt = requestsReceived.some(r => r.type === 'prompt');
    const hasSelect = requestsReceived.some(r => r.type === 'select');
    const hasConfirm = requestsReceived.some(r => r.type === 'confirm');

    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    if (hasPrompt && hasSelect && hasConfirm) {{
      console.log('  ✅ Bidirectional smoke test PASSED');
      console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
      console.log('All bidirectional request types handled:');
      console.log('  ✅ prompt');
      console.log('  ✅ select');
      console.log('  ✅ confirm');
    }} else {{
      console.log('  ❌ Bidirectional smoke test FAILED');
      console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
      console.log('Missing bidirectional requests:');
      if (!hasPrompt) console.log('  ❌ prompt');
      if (!hasSelect) console.log('  ❌ select');
      if (!hasConfirm) console.log('  ❌ confirm');
      process.exit(1);
    }}

  }} catch (err: any) {{
    if (!connected && (err.code === 'ECONNREFUSED' || err.message?.includes('connect'))) {{
      console.log(`\n⚠️  Server not running at ws://localhost:${{PORT}}`);
      console.log('   This is expected if the server is not started.');
      console.log('   Skipping bidirectional smoke test.\n');
      process.exit(0);
    }}

    console.error('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.error('  ❌ Bidirectional smoke test FAILED');
    console.error('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.error('\nError:', err.message || err);
    if (err.stack) {{
      console.error('\nStack trace:');
      console.error(err.stack);
    }}
    console.error('');
    process.exit(1);

  }} finally {{
    if (connected) {{
      client.disconnect();
      console.log('\nDisconnected from server.');
    }}
  }}
}}

// Run the test
bidirSmokeTest();
"#, backend = backend, import_line = import_line, backend_upper = backend.to_uppercase())
}
