// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { MethodStream } from '../rpc';

/** Typed client interface for tickets plugin */
export interface TicketsClient {
  /** Change a ticket's status */
  setStatus(id: string, status: string): MethodStream<'tickets.set_status', string>;
}

/** Typed client implementation for tickets plugin */
class TicketsClientImpl implements TicketsClient {
  private rpc: RpcClient;
  constructor(rpc: RpcClient) { this.rpc = rpc; }

  setStatus(id: string, status: string): MethodStream<'tickets.set_status', string> {
    const stream = this.rpc.call('tickets.set_status', { id, status });
    return new MethodStream<'tickets.set_status', string>('tickets.set_status', stream);
  }
}

/** Create a typed tickets client from an RPC client */
export function createTicketsClient(rpc: RpcClient): TicketsClient {
  return new TicketsClientImpl(rpc);
}