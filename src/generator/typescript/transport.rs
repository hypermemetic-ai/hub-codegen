//! WebSocket transport generation
//!
//! Generates a SubstrateClient class that implements RpcClient using
//! WebSocket + JSON-RPC 2.0 subscription protocol.

/// Generate the WebSocket transport implementation
pub fn generate_transport() -> String {
    r#"// Auto-generated WebSocket transport
// Implements RpcClient using JSON-RPC 2.0 subscription protocol

import type { RpcClient } from './rpc';
import type { PlexusStreamItem, StreamMetadata } from './types';
import WebSocket from 'ws';

/**
 * Configuration for SubstrateClient
 */
export interface SubstrateConfig {
  /** WebSocket URL (e.g., "ws://localhost:4444") */
  url: string;
  /** Connection timeout in milliseconds (default: 5000) */
  connectionTimeout?: number;
  /** Enable debug logging (default: false) */
  debug?: boolean;
}

/**
 * Default configuration for local development
 */
export const defaultConfig: SubstrateConfig = {
  url: 'ws://127.0.0.1:4444',
  connectionTimeout: 5000,
  debug: false,
};

/**
 * JSON-RPC 2.0 request structure
 */
interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: number;
  method: string;
  params?: unknown;
}

/**
 * JSON-RPC 2.0 response (success)
 */
interface JsonRpcSuccess {
  jsonrpc: '2.0';
  id: number;
  result: unknown;
}

/**
 * JSON-RPC 2.0 response (error)
 */
interface JsonRpcError {
  jsonrpc: '2.0';
  id: number;
  error: {
    code: number;
    message: string;
    data?: unknown;
  };
}

type JsonRpcResponse = JsonRpcSuccess | JsonRpcError;

/**
 * JSON-RPC 2.0 notification (subscription update)
 */
interface JsonRpcNotification {
  jsonrpc: '2.0';
  method: 'subscription';
  params: {
    subscription: number;
    result: PlexusStreamItem;
  };
}

/**
 * Pending request awaiting subscription ID response
 */
interface PendingRequest {
  resolve: (subscriptionId: number) => void;
  reject: (error: Error) => void;
}

/**
 * Active subscription with its event queue
 */
interface ActiveSubscription {
  queue: PlexusStreamItem[];
  waiting: ((item: PlexusStreamItem | null) => void) | null;
  done: boolean;
}

/**
 * WebSocket-based RPC client for Substrate
 *
 * Implements the JSON-RPC 2.0 subscription protocol:
 * 1. Send request, receive subscription ID
 * 2. Receive notifications with that subscription ID
 * 3. Stream terminates on done/error item
 */
export class SubstrateClient implements RpcClient {
  private ws: WebSocket | null = null;
  private nextId = 1;
  private pendingRequests = new Map<number, PendingRequest>();
  private subscriptions = new Map<number, ActiveSubscription>();
  private pendingSubscriptionMessages = new Map<number, PlexusStreamItem[]>();
  private config: Required<SubstrateConfig>;
  private connectionPromise: Promise<void> | null = null;

  constructor(config: Partial<SubstrateConfig> = {}) {
    this.config = {
      url: config.url ?? defaultConfig.url,
      connectionTimeout: config.connectionTimeout ?? defaultConfig.connectionTimeout ?? 5000,
      debug: config.debug ?? defaultConfig.debug ?? false,
    };
  }

  private log(...args: unknown[]): void {
    if (this.config.debug) {
      console.log('[SubstrateClient]', ...args);
    }
  }

  /**
   * Connect to the WebSocket server
   */
  async connect(): Promise<void> {
    if (this.ws?.readyState === WebSocket.OPEN) {
      return;
    }

    if (this.connectionPromise) {
      return this.connectionPromise;
    }

    this.connectionPromise = new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Connection timeout after ${this.config.connectionTimeout}ms`));
      }, this.config.connectionTimeout);

      this.ws = new WebSocket(this.config.url);

      this.ws.onopen = () => {
        clearTimeout(timeout);
        this.log('Connected to', this.config.url);
        resolve();
      };

      this.ws.onerror = (event) => {
        clearTimeout(timeout);
        this.log('WebSocket error:', event);
        reject(new Error('WebSocket connection failed'));
      };

      this.ws.onclose = (event) => {
        this.log('WebSocket closed:', event.code, event.reason);
        this.handleDisconnect();
      };

      this.ws.onmessage = (event) => {
        this.handleMessage(event.data.toString());
      };
    });

    try {
      await this.connectionPromise;
    } finally {
      this.connectionPromise = null;
    }
  }

  /**
   * Disconnect from the WebSocket server
   */
  disconnect(): void {
    if (this.ws) {
      this.ws.close(1000, 'Client disconnect');
      this.ws = null;
    }
    this.handleDisconnect();
  }

  private handleDisconnect(): void {
    // Reject all pending requests
    for (const [id, pending] of this.pendingRequests) {
      pending.reject(new Error('Connection closed'));
      this.pendingRequests.delete(id);
    }

    // Signal all subscriptions as done
    for (const [id, sub] of this.subscriptions) {
      sub.done = true;
      if (sub.waiting) {
        sub.waiting(null);
        sub.waiting = null;
      }
      this.subscriptions.delete(id);
    }
  }

  private handleMessage(data: string): void {
    this.log('Received:', data);

    let msg: unknown;
    try {
      msg = JSON.parse(data);
    } catch {
      this.log('Failed to parse message:', data);
      return;
    }

    const obj = msg as Record<string, unknown>;

    // Check if it's a notification (has method but no id)
    // Accept both 'subscription' (JSON-RPC 2.0 standard) and 'result' (Substrate variant)
    if ((obj.method === 'subscription' || obj.method === 'result') && !('id' in obj)) {
      this.handleNotification(msg as JsonRpcNotification);
      return;
    }

    // Otherwise it's a response (has id)
    if ('id' in obj) {
      this.handleResponse(msg as JsonRpcResponse);
      return;
    }

    this.log('Unknown message format:', msg);
  }

  private handleResponse(resp: JsonRpcResponse): void {
    const pending = this.pendingRequests.get(resp.id);
    if (!pending) {
      this.log('Unknown request id:', resp.id);
      return;
    }

    this.pendingRequests.delete(resp.id);

    if ('error' in resp) {
      pending.reject(new Error(`RPC error ${resp.error.code}: ${resp.error.message}`));
    } else {
      // Result is the subscription ID
      const subscriptionId = resp.result as number;
      pending.resolve(subscriptionId);
    }
  }

  private handleNotification(notif: JsonRpcNotification): void {
    const subscriptionId = notif.params.subscription;
    const item = notif.params.result;

    let sub = this.subscriptions.get(subscriptionId);
    if (!sub) {
      // Queue messages for subscriptions that haven't been registered yet
      if (!this.pendingSubscriptionMessages.has(subscriptionId)) {
        this.pendingSubscriptionMessages.set(subscriptionId, []);
      }
      this.pendingSubscriptionMessages.get(subscriptionId)!.push(item);
      return;
    }

    // Check if stream is terminating
    if (item.type === 'done' || item.type === 'error') {
      sub.done = true;
    }

    // Either deliver immediately or queue
    if (sub.waiting) {
      const waiter = sub.waiting;
      sub.waiting = null;
      waiter(item);
    } else {
      sub.queue.push(item);
    }

    // Clean up completed subscriptions
    if (sub.done && sub.queue.length === 0) {
      this.subscriptions.delete(subscriptionId);
    }
  }

  /**
   * Call a method and receive a stream of PlexusStreamItem responses.
   */
  async *call(method: string, params?: unknown): AsyncGenerator<PlexusStreamItem> {
    // Ensure connected
    await this.connect();

    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('Not connected');
    }

    // Create subscription first so we can capture early messages
    const sub: ActiveSubscription = {
      queue: [],
      waiting: null,
      done: false,
    };

    // Send request - wrap in plexus.call
    const id = this.nextId++;
    const request: JsonRpcRequest = {
      jsonrpc: '2.0',
      id,
      method: 'plexus.call',
      params: {
        method,
        params: params ?? {},
      },
    };

    this.log('Sending:', JSON.stringify(request));

    // Create promise for subscription ID
    const subscriptionIdPromise = new Promise<number>((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
    });

    this.ws.send(JSON.stringify(request));

    // Wait for subscription ID
    let subscriptionId: number;
    try {
      subscriptionId = await subscriptionIdPromise;
    } catch (err) {
      throw err;
    }

    this.log('Got subscription ID:', subscriptionId);

    // Now register subscription with the real ID
    this.subscriptions.set(subscriptionId, sub);

    // Flush any messages that arrived before registration
    const pendingMessages = this.pendingSubscriptionMessages.get(subscriptionId);
    if (pendingMessages) {
      this.pendingSubscriptionMessages.delete(subscriptionId);
      for (const msg of pendingMessages) {
        if (msg.type === 'done' || msg.type === 'error') {
          sub.done = true;
        }
        sub.queue.push(msg);
      }
    }

    // Yield items until done
    try {
      while (true) {
        // Check if there's already an item in the queue
        if (sub.queue.length > 0) {
          const item = sub.queue.shift()!;
          yield item;

          // Check for termination
          if (item.type === 'done' || item.type === 'error') {
            return;
          }
          continue;
        }

        // If stream is done and queue is empty, we're finished
        if (sub.done) {
          return;
        }

        // Wait for next item
        const item = await new Promise<PlexusStreamItem | null>((resolve) => {
          sub.waiting = resolve;
        });

        if (item === null) {
          // Connection closed
          return;
        }

        yield item;

        // Check for termination
        if (item.type === 'done' || item.type === 'error') {
          return;
        }
      }
    } finally {
      // Clean up subscription
      this.subscriptions.delete(subscriptionId);
    }
  }
}

/**
 * Create a SubstrateClient with the given configuration
 */
export function createClient(config: Partial<SubstrateConfig> = {}): SubstrateClient {
  return new SubstrateClient(config);
}
"#.to_string()
}
