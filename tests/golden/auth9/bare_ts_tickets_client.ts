// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { MethodStream } from '../rpc';
import type { TicketsEvent } from './events';
import type { Up } from '../rpc';

/** Typed client interface for tickets plugin */
export interface TicketsClient<A = undefined> {
  /** Change a ticket's status */
  setStatus(id: string, status: string): MethodStream<'tickets.set_status', string, Up<A, TicketsEvent>>;
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

  setStatus(id: string, status: string): MethodStream<'tickets.set_status', string, Up<A, TicketsEvent>> {
    const stream = this.rpc.call('tickets.set_status', { id, status });
    return new MethodStream<'tickets.set_status', string, Up<A, TicketsEvent>>('tickets.set_status', stream, (event: string) => this.__liftUp({ method: 'set_status', event } as unknown as TicketsEvent));
  }
}

/** Create a typed tickets client from an RPC client */
export function createTicketsClient<A = undefined>(rpc: RpcClient, lift?: (event: TicketsEvent) => A): TicketsClient<A> {
  return new TicketsClientImpl<A>(rpc, lift);
}