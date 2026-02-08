//! Smoke test generation
//!
//! Generates test/smoke.test.ts for verifying the client works.

use crate::ir::IR;

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
        throw new Error(`Backend returned error: ${{item.error}}`);
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
