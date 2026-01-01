//! Smoke test generation
//!
//! Generates test/smoke.test.ts for verifying the client works.

/// Generate smoke test content
pub fn generate_smoke_test() -> String {
    r#"// Auto-generated smoke test
// Run with: npm test

import { createClient, createHealthClient, createEchoClient } from '../index';

async function main() {
  console.log('Connecting to substrate...');
  const rpc = createClient({ url: 'ws://localhost:4444' });
  
  // Test health.check (non-streaming)
  console.log('\nTesting health.check...');
  const health = createHealthClient(rpc);
  const status = await health.check();
  console.log('✓ health.check:', status.type);
  
  // Test echo.once (non-streaming)
  console.log('\nTesting echo.once...');
  const echo = createEchoClient(rpc);
  const once = await echo.once('test message');
  console.log('✓ echo.once:', once.message);
  if (once.message !== 'test message') {
    throw new Error(`Expected 'test message', got '${once.message}'`);
  }
  
  // Test echo.echo (streaming)
  console.log('\nTesting echo.echo (streaming)...');
  let count = 0;
  for await (const event of echo.echo('streaming test', 3)) {
    count++;
    console.log(`  event ${count}:`, event);
  }
  console.log(`✓ echo.echo: received ${count} events`);
  if (count !== 3) {
    throw new Error(`Expected 3 events, got ${count}`);
  }
  
  rpc.disconnect();
  console.log('\n✅ All smoke tests passed!');
}

main().catch(err => {
  console.error('\n❌ Smoke test failed:', err);
  process.exit(1);
});
"#.to_string()
}
