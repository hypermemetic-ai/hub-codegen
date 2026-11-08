// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { MethodStream, collectOne } from '../rpc';
import type { ConfirmToken, NeedsInput } from '../types';
import type { ArchiveEvent, Deleted } from './types';
import type { TicketsEvent } from './events';
import type { Up } from '../rpc';

/** Typed client interface for tickets plugin */
export interface TicketsClient<A = undefined> {
  /**
   * Archive the tickets a filter matches
   * @requires `tickets.archive` anywhere it is held
   */
  archive(filter: string, confirm?: ConfirmToken): MethodStream<'tickets.archive', ArchiveEvent | NeedsInput, Up<A, TicketsEvent>>;
  /**
   * Delete tickets for good
   * @requires `tickets.delete` anywhere it is held
   */
  delete(ids: string[], children?: string, confirm?: ConfirmToken): Promise<Deleted | NeedsInput>;
  /**
   * One ticket (asks nothing)
   * @requires `tickets.get` anywhere it is held
   */
  get(id: string): Promise<Deleted>;
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

  archive(filter: string, confirm?: ConfirmToken): MethodStream<'tickets.archive', ArchiveEvent | NeedsInput, Up<A, TicketsEvent>> {
    const stream = this.rpc.call('tickets.archive', { filter, confirm });
    return new MethodStream<'tickets.archive', ArchiveEvent | NeedsInput, Up<A, TicketsEvent>>('tickets.archive', stream, (event: ArchiveEvent | NeedsInput) => this.__liftUp({ method: 'archive', event } as unknown as TicketsEvent));
  }

  async delete(ids: string[], children?: string, confirm?: ConfirmToken): Promise<Deleted | NeedsInput> {
    const stream = this.rpc.call('tickets.delete', { ids, children, confirm });
    return collectOne<Deleted | NeedsInput>(stream);
  }

  async get(id: string): Promise<Deleted> {
    const stream = this.rpc.call('tickets.get', { id });
    return collectOne<Deleted>(stream);
  }
}

/** Create a typed tickets client from an RPC client */
export function createTicketsClient<A = undefined>(rpc: RpcClient, lift?: (event: TicketsEvent) => A): TicketsClient<A> {
  return new TicketsClientImpl<A>(rpc, lift);
}

/** What each tickets method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */
export const requires = {
  "tickets.archive": { grant: "tickets.archive", target: null },
  "tickets.delete": { grant: "tickets.delete", target: null },
  "tickets.get": { grant: "tickets.get", target: null },
} as const;