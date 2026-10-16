// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { extractData, collectOne } from '../rpc';
import type { SpikeEvent, Status, TicketId } from './types';

/** Typed client interface for tickets plugin */
export interface TicketsClient {
  /**
   * Read one ticket.
   * @requires `tickets.get` anywhere it is held
   */
  get(id: TicketId): AsyncGenerator<SpikeEvent>;
  /**
   * Get plugin or method schema. Pass {"method": "name"} for a specific method.
   * @requires `tickets.schema` anywhere it is held
   */
  schema(): Promise<unknown>;
  /**
   * Move a ticket to a status.
   * @requires `tickets.set_status` anywhere it is held
   */
  setStatus(id: TicketId, status: Status): AsyncGenerator<SpikeEvent>;
}

/** Typed client implementation for tickets plugin */
class TicketsClientImpl implements TicketsClient {
  private rpc: RpcClient;
  constructor(rpc: RpcClient) { this.rpc = rpc; }

  async *get(id: TicketId): AsyncGenerator<SpikeEvent> {
    const stream = this.rpc.call('tickets.get', { id });
    yield* extractData<SpikeEvent>(stream);
  }

  async schema(): Promise<unknown> {
    const stream = this.rpc.call('tickets.schema', {});
    return collectOne<unknown>(stream);
  }

  async *setStatus(id: TicketId, status: Status): AsyncGenerator<SpikeEvent> {
    const stream = this.rpc.call('tickets.set_status', { id, status });
    yield* extractData<SpikeEvent>(stream);
  }
}

/** Create a typed tickets client from an RPC client */
export function createTicketsClient(rpc: RpcClient): TicketsClient {
  return new TicketsClientImpl(rpc);
}

/** What each tickets method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */
export const requires = {
  "tickets.get": { grant: "tickets.get", target: null },
  "tickets.schema": { grant: "tickets.schema", target: null },
  "tickets.set_status": { grant: "tickets.set_status", target: null },
} as const;