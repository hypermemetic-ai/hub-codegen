//! RPC client layer generation
//!
//! Generates the raw RPC client interface that returns PlexusStreamItem directly.
//! This is Layer 1 of the two-layer architecture.

/// Generate the RPC client interface and utilities
pub fn generate_rpc_client() -> String {
    r#"// Auto-generated RPC client interface
// This is Layer 1: raw RPC that returns PlexusStreamItem

import type { PlexusStreamItem } from './types';

/**
 * Raw RPC client interface for hub communication.
 *
 * This is the low-level transport layer. All methods return AsyncGenerator<PlexusStreamItem>.
 * Use the typed client wrappers for a better developer experience.
 */
export interface RpcClient {
  /**
   * Call a method and receive a stream of PlexusStreamItem responses.
   *
   * @param method - Fully qualified method name (e.g., "echo.once", "cone.chat")
   * @param params - Method parameters as a JSON-serializable object
   * @returns AsyncGenerator yielding PlexusStreamItem events
   */
  call(method: string, params?: unknown): AsyncGenerator<PlexusStreamItem>;
}

/**
 * Helper to extract data content from a PlexusStreamItem stream.
 * Throws PlexusError on error events.
 *
 * @param stream - AsyncGenerator of PlexusStreamItem
 * @returns AsyncGenerator of the unwrapped content (typed as T)
 */
export async function* extractData<T>(
  stream: AsyncGenerator<PlexusStreamItem>
): AsyncGenerator<T> {
  for await (const item of stream) {
    switch (item.type) {
      case 'data':
        yield item.content as T;
        break;
      case 'error':
        throw new PlexusError(
          item.message,
          item.code,
          item.recoverable,
          item.metadata
        );
      case 'progress':
        // Progress events are informational, skip
        break;
      case 'done':
        // Stream completed
        return;
    }
  }
}

/**
 * Helper to collect a single result from a non-streaming method.
 * Throws PlexusError on error events.
 * Throws if no data is received.
 *
 * @param stream - AsyncGenerator of PlexusStreamItem
 * @returns Promise of the unwrapped content (typed as T)
 */
export async function collectOne<T>(
  stream: AsyncGenerator<PlexusStreamItem>
): Promise<T> {
  for await (const item of stream) {
    switch (item.type) {
      case 'data':
        return item.content as T;
      case 'error':
        throw new PlexusError(
          item.message,
          item.code,
          item.recoverable,
          item.metadata
        );
      case 'progress':
        // Progress events are informational, skip
        break;
      case 'done':
        break;
    }
  }
  throw new Error('No data received from method call');
}

// Re-export PlexusError for convenience
export { PlexusError } from './types';
"#.to_string()
}
