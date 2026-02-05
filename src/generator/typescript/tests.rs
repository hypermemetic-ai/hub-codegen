//! Smoke test generation
//!
//! Generates test/smoke.test.ts for verifying the client works.

/// Generate smoke test content
pub fn generate_smoke_test() -> String {
    r#"// Auto-generated smoke test
// Run with: npm test

import { createClient } from '../transport';
import { Health, Echo } from '../index';

async function main() {
  console.log('Connecting to substrate...');
  const rpc = createClient({ url: 'ws://localhost:4444' });

  // Test health.check (non-streaming)
  console.log('\nTesting health.check...');
  const health = new Health.HealthClientImpl(rpc);
  const status = await health.check();
  console.log('✓ health.check:', status);

  // Test echo.once (non-streaming)
  console.log('\nTesting echo.once...');
  const echo = new Echo.EchoClientImpl(rpc);
  const once = await echo.once('test message');
  console.log('✓ echo.once:', once);
  if (once.message !== 'test message') {
    throw new Error(`Expected 'test message', got '${once.message}'`);
  }

  // Test echo.echo (non-streaming, count first)
  console.log('\nTesting echo.echo...');
  const echoResult = await echo.echo(3, 'test');
  console.log('✓ echo.echo:', echoResult);

  rpc.disconnect();
  console.log('\n✅ All smoke tests passed!');
}

main().catch(err => {
  console.error('\n❌ Smoke test failed:', err);
  process.exit(1);
});
"#.to_string()
}
