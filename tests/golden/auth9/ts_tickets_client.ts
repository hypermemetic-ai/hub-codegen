// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { MethodStream } from '../rpc';
import type { TicketsEvent } from './events';
import type { Up } from '../rpc';

/** Typed client interface for tickets plugin */
export interface TicketsClient<A = undefined> {
  /**
   * Change a ticket's status
   *
   * Moves it through Pending → Ready → Coding → In Review → Complete;
   * a doc has no lifecycle and is refused.
   * @requires `tickets.set_status` on the scope of `id`
   */
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

/** What each tickets method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */
export const requires = {
  "tickets.set_status": { grant: "tickets.set_status", target: "id" },
} as const;

/** The tickets activation's default roles, each the grants it lists (AUTH-3). Defaults only: a server's roles are data an admin may change, and `admin` holds every grant. */
export const defaultRoles = {
  "editor": ["tickets.get", "tickets.set_status"],
  "viewer": ["tickets.get"],
} as const;