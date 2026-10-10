// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { collectOne } from '../rpc';

/** Typed client interface for comments plugin */
export interface CommentsClient {
  /**
   * Comments, newest first
   * @requires `comments.list` anywhere it is held
   */
  list(): Promise<string>;
}

/** Typed client implementation for comments plugin */
class CommentsClientImpl implements CommentsClient {
  private rpc: RpcClient;
  constructor(rpc: RpcClient) { this.rpc = rpc; }

  async list(): Promise<string> {
    const stream = this.rpc.call('comments.list', {});
    return collectOne<string>(stream);
  }
}

/** Create a typed comments client from an RPC client */
export function createCommentsClient(rpc: RpcClient): CommentsClient {
  return new CommentsClientImpl(rpc);
}

/** What each comments method needs (AUTH-9): its grant, and the param that locates what it acts on (`null`: allowed wherever the grant is held). The gate decides, server-side. */
export const requires = {
  "comments.list": { grant: "comments.list", target: null },
} as const;