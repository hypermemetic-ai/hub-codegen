// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { MethodStream, collectOne } from '../rpc';
import type { SpikeEvent, Status, TicketId } from './types';
import type { TicketsEvent } from './events';
import type { Up } from '../rpc';

/** Typed client interface for tickets plugin */
export interface TicketsClient<A = undefined> {
  /**
   * Read one ticket.
   * @requires `tickets.get` anywhere it is held
   */
  get(id: TicketId): MethodStream<'tickets.get', SpikeEvent, Up<A, TicketsEvent>>;
  /**
   * Get plugin or method schema. Pass {"method": "name"} for a specific method.
   * @requires `tickets.schema` anywhere it is held
   */
  schema(): Promise<unknown>;
  /**
   * Move a ticket to a status.
   * @requires `tickets.set_status` anywhere it is held
   */
  setStatus(id: TicketId, status: Status): MethodStream<'tickets.set_status', SpikeEvent, Up<A, TicketsEvent>>;
}

/** Typed client implementation for tickets plugin */
class TicketsClientImpl<A = undefined> implements TicketsClient<A> {
  private rpc: RpcClient;
  private readonly __lift: ((event: TicketsEvent) => A) | undefined;
  constructor(rpc: RpcClient, lift?: (event: TicketsEvent) => A) {
    this.rpc = rpc;
    this.__lift = lift;
  }

  private __liftUp(event: TicketsEvent): Up<A, TicketsEvent> {
    return (this.__lift ? this.__lift(event) : event) as Up<A, TicketsEvent>;
  }

  get(id: TicketId): MethodStream<'tickets.get', SpikeEvent, Up<A, TicketsEvent>> {
    const stream = this.rpc.call('tickets.get', { id });
    return new MethodStream<'tickets.get', SpikeEvent, Up<A, TicketsEvent>>('tickets.get', stream, (event: SpikeEvent) => this.__liftUp({ method: 'get', event } as unknown as TicketsEvent));
  }

  async schema(): Promise<unknown> {
    const stream = this.rpc.call('tickets.schema', {});
    return collectOne<unknown>(stream);
  }

  setStatus(id: TicketId, status: Status): MethodStream<'tickets.set_status', SpikeEvent, Up<A, TicketsEvent>> {
    const stream = this.rpc.call('tickets.set_status', { id, status });
    return new MethodStream<'tickets.set_status', SpikeEvent, Up<A, TicketsEvent>>('tickets.set_status', stream, (event: SpikeEvent) => this.__liftUp({ method: 'set_status', event } as unknown as TicketsEvent));
  }
}

/** Create a typed tickets client from an RPC client */
export function createTicketsClient<A = undefined>(rpc: RpcClient, lift?: (event: TicketsEvent) => A): TicketsClient<A> {
  return new TicketsClientImpl<A>(rpc, lift);
}

/** What each tickets method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */
export const requires = {
  "tickets.get": { grant: "tickets.get", target: null },
  "tickets.schema": { grant: "tickets.schema", target: null },
  "tickets.set_status": { grant: "tickets.set_status", target: null },
} as const;