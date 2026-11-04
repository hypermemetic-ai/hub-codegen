// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { MethodStream, collectOne } from '../rpc';
import { pullInput, pushInput } from '../input';
import type { DuplexCall, InputOptions } from '../input';
import type { IngestEvent, Item } from './types';

/** Typed client interface for ingest plugin */
export interface IngestClient {
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
  tokens(run: string, limit: number | undefined, meta: Record<string, string> | undefined, input: AsyncIterable<string> | Iterable<string>, options?: InputOptions): MethodStream<'ingest.tokens', IngestEvent>;
  /** The push form of `tokens`: write the input as it comes — never refused, never dropped — and read `output`. */
  openTokens(run: string, limit?: number, meta?: Record<string, string>, options?: InputOptions): DuplexCall<string, MethodStream<'ingest.tokens', IngestEvent>>;
}

/** Typed client implementation for ingest plugin */
class IngestClientImpl implements IngestClient {
  private rpc: RpcClient;
  constructor(rpc: RpcClient) { this.rpc = rpc; }

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

  tokens(run: string, limit: number | undefined, meta: Record<string, string> | undefined, input: AsyncIterable<string> | Iterable<string>, options?: InputOptions): MethodStream<'ingest.tokens', IngestEvent> {
    const stream = pullInput<string>(this.rpc, 'ingest.tokens', { run, limit, meta }, false, input, options);
    return new MethodStream<'ingest.tokens', IngestEvent>('ingest.tokens', stream);
  }

  openTokens(run: string, limit?: number, meta?: Record<string, string>, options?: InputOptions): DuplexCall<string, MethodStream<'ingest.tokens', IngestEvent>> {
    return pushInput(this.rpc, 'ingest.tokens', { run, limit, meta }, false, options, (stream) => new MethodStream<'ingest.tokens', IngestEvent>('ingest.tokens', stream));
  }
}

/** Create a typed ingest client from an RPC client */
export function createIngestClient(rpc: RpcClient): IngestClient {
  return new IngestClientImpl(rpc);
}

/** What each ingest method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */
export const requires = {
  "ingest.bytes": { grant: "ingest.bytes", target: null },
  "ingest.items": { grant: "ingest.items", target: null },
  "ingest.tokens": { grant: "ingest.tokens", target: null },
} as const;