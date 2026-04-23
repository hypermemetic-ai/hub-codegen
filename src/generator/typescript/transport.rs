//! WebSocket transport generation
//!
//! Generates the WebSocket transport class. Imports protocol types from ./types
//! and the RpcClient interface from ./rpc — no duplication across the three files.

use crate::generator::TransportEnv;

/// Generate the WebSocket transport implementation
pub fn generate_transport(env: TransportEnv) -> String {
    let template = get_transport_template();
    if env == TransportEnv::Browser {
        template.lines()
            .filter(|l| *l != "import WebSocket from 'ws';")
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        template
    }
}

fn get_transport_template() -> String {
    r#"// Plexus WebSocket transport
// SAFE-7-cookie-auth-marker: this transport uses cookies for JWT auth on WS upgrade.
// URL-query token construction is intentionally NOT supported — plexus-transport removed
// that path. Set the access_token cookie via Node's headers option (below) or, in
// browsers, ensure the same-origin cookie store has access_token set before connect().
//
// Depends on ./types (protocol types) and ./rpc (RpcClient interface + helpers).
import WebSocket from 'ws';
import type {
  PlexusStreamItem,
  PlexusStreamItemRequest,
  StandardRequest,
  StandardResponse,
} from './types';
import type { RpcClient } from './rpc';

// ─── WebSocket transport ───────────────────────────────────────────────────

export interface PlexusRpcConfig {
  backend: string;
  url: string;
  connectionTimeout?: number;
  debug?: boolean;
  onBidirectionalRequest?: BidirectionalRequestHandler;
  /**
   * JWT auth token to attach as a Cookie on the WebSocket upgrade.
   *
   * - Node.js: passed as `Cookie: access_token=<jwt>` upgrade header.
   * - Browser: ignored — the browser's WebSocket API does not allow custom headers.
   *   Browsers must set `document.cookie` for the same origin before calling connect();
   *   the cookie is then sent automatically.
   *
   * SAFE-7: cookie-only WS auth migration. URL-query token paths are not supported.
   */
  authToken?: string;
}

export type BidirectionalRequestHandler = (
  request: StandardRequest
) => Promise<StandardResponse | undefined>;

interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: number;
  method: string;
  params?: unknown;
}

interface JsonRpcSuccess { jsonrpc: '2.0'; id: number; result: unknown; }
interface JsonRpcError   { jsonrpc: '2.0'; id: number; error: { code: number; message: string; data?: unknown }; }
type JsonRpcResponse = JsonRpcSuccess | JsonRpcError;

// ─── Typed RPC errors (REQ-7) ────────────────────────────────────────────
//
// Semantic JSON-RPC error codes per plexus-core's `plexus_error_to_jsonrpc`:
//   -32001  Authentication required
//   -32602  Invalid parameters
//   -32601  Method not found
//   -32000  Server-side execution error
// Transport dispatches to the appropriate subclass so client code can
// `catch (e) { if (e instanceof AuthenticationError) … }` rather than
// string-match on error messages.

export class PlexusRpcError extends Error {
  readonly code: number;
  readonly data?: unknown;
  constructor(code: number, message: string, data?: unknown) {
    super(`RPC error ${code}: ${message}`);
    this.name = 'PlexusRpcError';
    this.code = code;
    this.data = data;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class AuthenticationError extends PlexusRpcError {
  constructor(message: string, data?: unknown) { super(-32001, message, data); this.name = 'AuthenticationError'; }
}
export class InvalidParamsError extends PlexusRpcError {
  constructor(message: string, data?: unknown) { super(-32602, message, data); this.name = 'InvalidParamsError'; }
}
export class MethodNotFoundError extends PlexusRpcError {
  constructor(message: string, data?: unknown) { super(-32601, message, data); this.name = 'MethodNotFoundError'; }
}
export class ExecutionError extends PlexusRpcError {
  constructor(message: string, data?: unknown) { super(-32000, message, data); this.name = 'ExecutionError'; }
}

/** Construct the appropriate typed error for a JSON-RPC error payload. */
function rpcErrorFor(code: number, message: string, data?: unknown): PlexusRpcError {
  switch (code) {
    case -32001: return new AuthenticationError(message, data);
    case -32602: return new InvalidParamsError(message, data);
    case -32601: return new MethodNotFoundError(message, data);
    case -32000: return new ExecutionError(message, data);
    default:     return new PlexusRpcError(code, message, data);
  }
}

interface JsonRpcNotification {
  jsonrpc: '2.0';
  method: 'subscription';
  params: { subscription: number; result: PlexusStreamItem };
}

interface PendingRequest {
  resolve: (subscriptionId: number) => void;
  reject: (error: Error) => void;
}

interface ActiveSubscription {
  queue: PlexusStreamItem[];
  waiting: ((item: PlexusStreamItem | null) => void) | null;
  done: boolean;
}

export class PlexusRpcClient implements RpcClient {
  private ws: WebSocket | null = null;
  private nextId = 1;
  private pendingRequests = new Map<number, PendingRequest>();
  private subscriptions = new Map<number, ActiveSubscription>();
  private pendingSubscriptionMessages = new Map<number, PlexusStreamItem[]>();
  private config: Omit<Required<PlexusRpcConfig>, 'onBidirectionalRequest'>;
  private connectionPromise: Promise<void> | null = null;
  private onBidirectionalRequest?: BidirectionalRequestHandler;

  constructor(config: PlexusRpcConfig) {
    this.config = {
      backend: config.backend,
      url: config.url,
      connectionTimeout: config.connectionTimeout ?? 5000,
      debug: config.debug ?? false,
      authToken: config.authToken ?? '',
    };
    this.onBidirectionalRequest = config.onBidirectionalRequest;
  }

  setBidirectionalHandler(handler: BidirectionalRequestHandler | undefined): void {
    this.onBidirectionalRequest = handler;
  }

  private log(...args: unknown[]): void {
    if (this.config.debug) console.log('[PlexusRpcClient]', ...args);
  }

  async connect(): Promise<void> {
    if (this.ws?.readyState === WebSocket.OPEN) return;
    if (this.connectionPromise) return this.connectionPromise;

    this.connectionPromise = new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Connection timeout after ${this.config.connectionTimeout}ms`));
      }, this.config.connectionTimeout);

      // SAFE-7: pass auth token as Cookie header on WS upgrade (Node only).
      // In browsers, the WebSocket constructor only accepts (url, protocols) — the
      // 'ws' library wraps native WebSocket and ignores extra args, so this is safe.
      const wsOpts = this.config.authToken
        ? ({ headers: { Cookie: 'access_token=' + this.config.authToken } } as any)
        : undefined;
      this.ws = new WebSocket(this.config.url, wsOpts);
      this.ws.onopen  = () => { clearTimeout(timeout); this.log('Connected to', this.config.url); resolve(); };
      this.ws.onerror = (event) => { clearTimeout(timeout); this.log('WebSocket error:', event); reject(new Error('WebSocket connection failed')); };
      this.ws.onclose = (event) => { this.log('WebSocket closed:', event.code, event.reason); this.handleDisconnect(); };
      this.ws.onmessage = (event) => { this.handleMessage(event.data.toString()); };
    });

    try { await this.connectionPromise; } finally { this.connectionPromise = null; }
  }

  disconnect(): void {
    if (this.ws) { this.ws.close(1000, 'Client disconnect'); this.ws = null; }
    this.handleDisconnect();
  }

  private handleDisconnect(): void {
    for (const [id, pending] of this.pendingRequests) { pending.reject(new Error('Connection closed')); this.pendingRequests.delete(id); }
    for (const [id, sub] of this.subscriptions) { sub.done = true; if (sub.waiting) { sub.waiting(null); sub.waiting = null; } this.subscriptions.delete(id); }
  }

  private handleMessage(data: string): void {
    this.log('Received:', data);
    let msg: unknown;
    try { msg = JSON.parse(data); } catch { this.log('Failed to parse message:', data); return; }
    const obj = msg as Record<string, unknown>;
    if ('method' in obj && !('id' in obj) && obj.params && typeof (obj.params as any).subscription !== 'undefined') {
      this.handleNotification(msg as JsonRpcNotification); return;
    }
    if ('id' in obj) { this.handleResponse(msg as JsonRpcResponse); return; }
    this.log('Unknown message format:', msg);
  }

  private handleResponse(resp: JsonRpcResponse): void {
    const pending = this.pendingRequests.get(resp.id);
    if (!pending) { this.log('Unknown request id:', resp.id); return; }
    this.pendingRequests.delete(resp.id);
    if ('error' in resp) { pending.reject(rpcErrorFor(resp.error.code, resp.error.message, resp.error.data)); }
    else { pending.resolve(resp.result as number); }
  }

  private handleNotification(notif: JsonRpcNotification): void {
    const subscriptionId = notif.params.subscription;
    const item = notif.params.result;
    let sub = this.subscriptions.get(subscriptionId);
    if (!sub) {
      if (!this.pendingSubscriptionMessages.has(subscriptionId)) this.pendingSubscriptionMessages.set(subscriptionId, []);
      this.pendingSubscriptionMessages.get(subscriptionId)!.push(item);
      return;
    }
    if (item.type === 'request') { this.handleBidirectionalRequest(item as PlexusStreamItemRequest); return; }
    if (item.type === 'done' || item.type === 'error') sub.done = true;
    if (sub.waiting) { const w = sub.waiting; sub.waiting = null; w(item); }
    else { sub.queue.push(item); }
    if (sub.done && sub.queue.length === 0) this.subscriptions.delete(subscriptionId);
  }

  private async handleBidirectionalRequest(requestItem: PlexusStreamItemRequest): Promise<void> {
    const { requestId, requestData, timeoutMs } = requestItem;
    if (!this.onBidirectionalRequest) {
      this.log('No bidirectional handler, auto-cancelling:', requestId);
      await this.sendBidirectionalResponse(requestId, { type: 'cancelled' }); return;
    }
    const timeoutPromise = new Promise<undefined>(resolve => setTimeout(() => resolve(undefined), timeoutMs));
    try {
      const response = await Promise.race([this.onBidirectionalRequest(requestData), timeoutPromise]);
      await this.sendBidirectionalResponse(requestId, response ?? { type: 'cancelled' });
    } catch (err) {
      this.log('Bidirectional handler error:', err);
      await this.sendBidirectionalResponse(requestId, { type: 'cancelled' });
    }
  }

  private async sendBidirectionalResponse(requestId: string, response: StandardResponse): Promise<void> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) { this.log('Cannot send response, not connected'); return; }
    const id = this.nextId++;
    this.ws.send(JSON.stringify({ jsonrpc: '2.0', id, method: `${this.config.backend}.respond`, params: { request_id: requestId, response_data: response } }));
  }

  async *call(method: string, params?: unknown): AsyncGenerator<PlexusStreamItem> {
    await this.connect();
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) throw new Error('Not connected');

    const sub: ActiveSubscription = { queue: [], waiting: null, done: false };
    const id = this.nextId++;
    const request: JsonRpcRequest = {
      jsonrpc: '2.0', id,
      method: `${this.config.backend}.call`,
      params: { method, params: params ?? {} },
    };
    this.log('Sending:', JSON.stringify(request));

    const subscriptionIdPromise = new Promise<number>((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
    });
    this.ws.send(JSON.stringify(request));

    const subscriptionId = await subscriptionIdPromise;
    this.log('Got subscription ID:', subscriptionId);
    this.subscriptions.set(subscriptionId, sub);

    const pendingMessages = this.pendingSubscriptionMessages.get(subscriptionId);
    if (pendingMessages) {
      this.pendingSubscriptionMessages.delete(subscriptionId);
      for (const msg of pendingMessages) {
        if (msg.type === 'done' || msg.type === 'error') sub.done = true;
        sub.queue.push(msg);
      }
    }

    try {
      while (true) {
        if (sub.queue.length > 0) {
          const item = sub.queue.shift()!;
          yield item;
          if (item.type === 'done' || item.type === 'error') return;
          continue;
        }
        if (sub.done) return;
        const item = await new Promise<PlexusStreamItem | null>(resolve => { sub.waiting = resolve; });
        if (item === null) return;
        yield item;
        if (item.type === 'done' || item.type === 'error') return;
      }
    } finally {
      this.subscriptions.delete(subscriptionId);
    }
  }
}

export function createClient(config: PlexusRpcConfig): PlexusRpcClient {
  return new PlexusRpcClient(config);
}
"#.to_string()
}
