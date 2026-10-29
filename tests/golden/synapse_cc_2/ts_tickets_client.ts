// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { MethodStream, collectOne } from '../rpc';
import type { SpikeEvent, Status, TicketId } from './types';
import type { TicketsInputsClient } from '../tickets/inputs/client';
import { createTicketsInputsClient } from '../tickets/inputs/client';

/** Typed client interface for tickets plugin */
export interface TicketsClient {
  /** The inputs namespace — `tickets.inputs.<method>`. */
  readonly inputs: TicketsInputsClient;
  /**
   * Read one ticket.
   * @server-derived auth
   * @reads-header origin
   * @requiresAuth — resolver: self.access.verify
   * @requires `tickets.get` anywhere it is held
   */
  get(id: TicketId): MethodStream<'tickets.get', SpikeEvent>;
  /**
   * Get plugin or method schema. Pass {"method": "name"} for a specific method.
   * @requires `tickets.schema` anywhere it is held
   */
  schema(): Promise<unknown>;
  /**
   * Move a ticket to a status.
   * @server-derived auth
   * @reads-header origin
   * @requiresAuth — resolver: self.access.verify
   * @requires `tickets.set_status` anywhere it is held
   */
  setStatus(id: TicketId, status: Status): MethodStream<'tickets.set_status', SpikeEvent>;
}

/** Typed client implementation for tickets plugin */
class TicketsClientImpl implements TicketsClient {
  private rpc: RpcClient;
  readonly inputs: TicketsInputsClient;
  constructor(rpc: RpcClient) {
    this.rpc = rpc;
    this.inputs = createTicketsInputsClient(rpc);
  }

  get(id: TicketId): MethodStream<'tickets.get', SpikeEvent> {
    const stream = this.rpc.call('tickets.get', { id });
    return new MethodStream<'tickets.get', SpikeEvent>('tickets.get', stream);
  }

  async schema(): Promise<unknown> {
    const stream = this.rpc.call('tickets.schema', {});
    return collectOne<unknown>(stream);
  }

  setStatus(id: TicketId, status: Status): MethodStream<'tickets.set_status', SpikeEvent> {
    const stream = this.rpc.call('tickets.set_status', { id, status });
    return new MethodStream<'tickets.set_status', SpikeEvent>('tickets.set_status', stream);
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