// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { collectOne } from '../rpc';

/** Typed client interface for echo plugin */
export interface EchoClient {
  /**
   * Liveness check
   * @public exempt from auth — no credential required
   * @authPosture required
   */
  ping(): Promise<string>;
}

/** Typed client implementation for echo plugin */
class EchoClientImpl implements EchoClient {
  private rpc: RpcClient;
  constructor(rpc: RpcClient) { this.rpc = rpc; }

  async ping(): Promise<string> {
    const stream = this.rpc.call('echo.ping', {});
    return collectOne<string>(stream);
  }
}

/** Create a typed echo client from an RPC client */
export function createEchoClient(rpc: RpcClient): EchoClient {
  return new EchoClientImpl(rpc);
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

/** Credential requirements for echo methods (R-4). Surfacing only — the gate enforces server-side. */
export const EchoMethodAuth: { readonly [method: string]: MethodAuthMetadata } = {
  ping: { public: true, authPosture: 'required' },
};