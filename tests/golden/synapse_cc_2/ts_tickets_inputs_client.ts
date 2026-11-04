// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../../rpc';
import { MethodStream, collectOne } from '../../rpc';
import type { SpikeEvent, Status, TicketId } from '../../tickets/types';
import type { TicketsInputsEvent } from './events';
import type { Up } from '../../rpc';

/** Typed client interface for tickets.inputs plugin */
export interface TicketsInputsClient<A = undefined> {
  /**
   * Answer an input.
   * @server-derived auth
   * @reads-header origin
   * @requiresAuth — resolver: self.access.verify
   * @requires `inputs.answer` anywhere it is held
   */
  answer(ticket: TicketId, value: Status): MethodStream<'tickets.inputs.answer', SpikeEvent, Up<A, TicketsInputsEvent>>;
  /**
   * A ticket's inputs.
   * @server-derived auth
   * @reads-header origin
   * @requiresAuth — resolver: self.access.verify
   * @requires `inputs.list` anywhere it is held
   */
  list(ticket: TicketId): MethodStream<'tickets.inputs.list', SpikeEvent, Up<A, TicketsInputsEvent>>;
  /**
   * Get plugin or method schema. Pass {"method": "name"} for a specific method.
   * @requires `inputs.schema` anywhere it is held
   */
  schema(): Promise<unknown>;
}

/** Typed client implementation for tickets.inputs plugin */
class TicketsInputsClientImpl<A = undefined> implements TicketsInputsClient<A> {
  private rpc: RpcClient;
  private readonly __lift: ((event: TicketsInputsEvent) => A) | undefined;
  constructor(rpc: RpcClient, lift?: (event: TicketsInputsEvent) => A) {
    this.rpc = rpc;
    this.__lift = lift;
  }

  private __liftUp(event: TicketsInputsEvent): Up<A, TicketsInputsEvent> {
    return (this.__lift ? this.__lift(event) : event) as Up<A, TicketsInputsEvent>;
  }

  answer(ticket: TicketId, value: Status): MethodStream<'tickets.inputs.answer', SpikeEvent, Up<A, TicketsInputsEvent>> {
    const stream = this.rpc.call('tickets.inputs.answer', { ticket, value });
    return new MethodStream<'tickets.inputs.answer', SpikeEvent, Up<A, TicketsInputsEvent>>('tickets.inputs.answer', stream, (event: SpikeEvent) => this.__liftUp({ method: 'answer', event } as unknown as TicketsInputsEvent));
  }

  list(ticket: TicketId): MethodStream<'tickets.inputs.list', SpikeEvent, Up<A, TicketsInputsEvent>> {
    const stream = this.rpc.call('tickets.inputs.list', { ticket });
    return new MethodStream<'tickets.inputs.list', SpikeEvent, Up<A, TicketsInputsEvent>>('tickets.inputs.list', stream, (event: SpikeEvent) => this.__liftUp({ method: 'list', event } as unknown as TicketsInputsEvent));
  }

  async schema(): Promise<unknown> {
    const stream = this.rpc.call('tickets.inputs.schema', {});
    return collectOne<unknown>(stream);
  }
}

/** Create a typed tickets.inputs client from an RPC client */
export function createTicketsInputsClient<A = undefined>(rpc: RpcClient, lift?: (event: TicketsInputsEvent) => A): TicketsInputsClient<A> {
  return new TicketsInputsClientImpl<A>(rpc, lift);
}

/** What each tickets.inputs method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */
export const requires = {
  "inputs.answer": { grant: "inputs.answer", target: null },
  "inputs.list": { grant: "inputs.list", target: null },
  "inputs.schema": { grant: "inputs.schema", target: null },
} as const;