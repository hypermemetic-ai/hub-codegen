// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { collectOne } from '../rpc';

/** Typed client interface for cone plugin */
export interface ConeClient {
  /**
   * Send a message
   * @requiresCredential kind: oauth_access, scopes: [facet.write, facet.read], site: header:authorization
   * @authPosture required
   */
  sendMessage(): Promise<string>;
}

/** Typed client implementation for cone plugin */
class ConeClientImpl implements ConeClient {
  private rpc: RpcClient;
  constructor(rpc: RpcClient) { this.rpc = rpc; }

  async sendMessage(): Promise<string> {
    const stream = this.rpc.call('cone.send_message', {});
    return collectOne<string>(stream);
  }
}

/** Create a typed cone client from an RPC client */
export function createConeClient(rpc: RpcClient): ConeClient {
  return new ConeClientImpl(rpc);
}

/** Per-method credential-requirement metadata (R-4). Surfacing only — enforcement is server-side. */
export interface MethodAuthMetadata {
  /** Credential the caller must hold. Absent = no scope-derived requirement. */
  readonly requiresCredential?: {
    /** Required credential kind (e.g. 'bearer', 'oauth_access'). Absent = any kind whose scopes match. */
    readonly kind?: string;
    /** Required scope set — the caller must satisfy ALL listed scopes. */
    readonly scopes: readonly string[];
    /** Preferred attach site (advisory), e.g. 'header:authorization'. */
    readonly siteHint?: string;
  };
  /** Explicitly public — exempt from the default-deny gate. */
  readonly public?: boolean;
  /** Declared auth posture of the owning activation. */
  readonly authPosture?: 'required' | 'optional' | 'mixed' | 'none';
}

/** Credential requirements for cone methods (R-4). Surfacing only — the gate enforces server-side. */
export const ConeMethodAuth: { readonly [method: string]: MethodAuthMetadata } = {
  sendMessage: { requiresCredential: { kind: 'oauth_access', scopes: ['facet.write', 'facet.read'], siteHint: 'header:authorization' }, authPosture: 'required' },
};