// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { collectOne } from '../rpc';
import type { Stat } from './types';

/** Typed client interface for bench plugin */
export interface BenchClient {
  /**
   * What the server has served.
   * @requires `bench.stats` anywhere it is held
   */
  stats(): Promise<Stat>;
}

/** Typed client implementation for bench plugin */
class BenchClientImpl implements BenchClient {
  private rpc: RpcClient;
  constructor(rpc: RpcClient) { this.rpc = rpc; }

  async stats(): Promise<Stat> {
    const stream = this.rpc.call('bench.stats', {});
    return collectOne<Stat>(stream);
  }
}

/** Create a typed bench client from an RPC client */
export function createBenchClient(rpc: RpcClient): BenchClient {
  return new BenchClientImpl(rpc);
}

/** What each bench method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */
export const requires = {
  "bench.stats": { grant: "bench.stats", target: null },
} as const;