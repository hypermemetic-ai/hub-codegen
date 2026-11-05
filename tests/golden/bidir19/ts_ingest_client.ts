// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { MethodStream, collectOne } from '../rpc';
import { pullInput, pushInput } from '../input';
import type { DuplexCall, InputOptions } from '../input';
import type { IngestEvent, Item } from './types';
import type { IngestAggregate } from './events';
import type { Up } from '../rpc';

/** Typed client interface for ingest plugin */
export interface IngestClient<A = undefined> {
  /**
   * BIDIR-17's raw ingress: chunks as they come, each beginning with
when it was sent (CLOCK_MONOTONIC ns, big-endian). The method only
consumes: binary frames and base64 JSON reach it alike.
   * @requires `ingest.bytes` anywhere it is held
   * @param input the input half (plexus-wire spec/10): pulled only while the service's credit allows, each item one binary frame
   */
  bytes(input: AsyncIterable<Uint8Array> | Iterable<Uint8Array>, options?: InputOptions): Promise<IngestEvent>;
  /** The push form of `bytes`: write the input as it comes — never refused, never dropped — and read `output`. */
  openBytes(options?: InputOptions): DuplexCall<Uint8Array, Promise<IngestEvent>>;
  /**
   * Take items as they come; answer how late they came.
   * @requires `ingest.items` anywhere it is held
   * @param input the input half (plexus-wire spec/10): pulled only while the service's credit allows, batched into `input` frames
   */
  items(input: AsyncIterable<Item> | Iterable<Item>, options?: InputOptions): Promise<IngestEvent>;
  /** The push form of `items`: write the input as it comes — never refused, never dropped — and read `output`. */
  openItems(options?: InputOptions): DuplexCall<Item, Promise<IngestEvent>>;
  /**
   * Count tokens as they come, one item a running count.
   * @requires `ingest.tokens` anywhere it is held
   * @param input the input half (plexus-wire spec/10): pulled only while the service's credit allows, batched into `input` frames
   */
  tokens(run: string, limit: number | undefined, meta: Record<string, string> | undefined, input: AsyncIterable<string> | Iterable<string>, options?: InputOptions): MethodStream<'ingest.tokens', IngestEvent, Up<A, IngestAggregate>>;
  /** The push form of `tokens`: write the input as it comes — never refused, never dropped — and read `output`. */
  openTokens(run: string, limit?: number, meta?: Record<string, string>, options?: InputOptions): DuplexCall<string, MethodStream<'ingest.tokens', IngestEvent, Up<A, IngestAggregate>>>;
}

/** Typed client implementation for ingest plugin */
class IngestClientImpl<A = undefined> implements IngestClient<A> {
  private rpc: RpcClient;
  private readonly __lift: ((event: IngestAggregate) => A) | undefined;
  constructor(rpc: RpcClient, lift?: (event: IngestAggregate) => A) {
    this.rpc = rpc;
    this.__lift = lift;
  }

  private __liftUp(event: IngestAggregate): Up<A, IngestAggregate> {
    return (this.__lift ? this.__lift(event) : event) as Up<A, IngestAggregate>;
  }

  bytes(input: AsyncIterable<Uint8Array> | Iterable<Uint8Array>, options?: InputOptions): Promise<IngestEvent> {
    const stream = pullInput<Uint8Array>(this.rpc, 'ingest.bytes', {}, true, input, options);
    return collectOne<IngestEvent>(stream);
  }

  openBytes(options?: InputOptions): DuplexCall<Uint8Array, Promise<IngestEvent>> {
    return pushInput(this.rpc, 'ingest.bytes', {}, true, options, (stream) => collectOne<IngestEvent>(stream));
  }

  items(input: AsyncIterable<Item> | Iterable<Item>, options?: InputOptions): Promise<IngestEvent> {
    const stream = pullInput<Item>(this.rpc, 'ingest.items', {}, false, input, options);
    return collectOne<IngestEvent>(stream);
  }

  openItems(options?: InputOptions): DuplexCall<Item, Promise<IngestEvent>> {
    return pushInput(this.rpc, 'ingest.items', {}, false, options, (stream) => collectOne<IngestEvent>(stream));
  }

  tokens(run: string, limit: number | undefined, meta: Record<string, string> | undefined, input: AsyncIterable<string> | Iterable<string>, options?: InputOptions): MethodStream<'ingest.tokens', IngestEvent, Up<A, IngestAggregate>> {
    const stream = pullInput<string>(this.rpc, 'ingest.tokens', { run, limit, meta }, false, input, options);
    return new MethodStream<'ingest.tokens', IngestEvent, Up<A, IngestAggregate>>('ingest.tokens', stream, (event: IngestEvent) => this.__liftUp({ method: 'tokens', event } as unknown as IngestAggregate));
  }

  openTokens(run: string, limit?: number, meta?: Record<string, string>, options?: InputOptions): DuplexCall<string, MethodStream<'ingest.tokens', IngestEvent, Up<A, IngestAggregate>>> {
    return pushInput(this.rpc, 'ingest.tokens', { run, limit, meta }, false, options, (stream) => new MethodStream<'ingest.tokens', IngestEvent, Up<A, IngestAggregate>>('ingest.tokens', stream, (event: IngestEvent) => this.__liftUp({ method: 'tokens', event } as unknown as IngestAggregate)));
  }
}

/** Create a typed ingest client from an RPC client */
export function createIngestClient<A = undefined>(rpc: RpcClient, lift?: (event: IngestAggregate) => A): IngestClient<A> {
  return new IngestClientImpl<A>(rpc, lift);
}

/** What each ingest method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */
export const requires = {
  "ingest.bytes": { grant: "ingest.bytes", target: null },
  "ingest.items": { grant: "ingest.items", target: null },
  "ingest.tokens": { grant: "ingest.tokens", target: null },
} as const;