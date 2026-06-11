// Auto-generated typed client (Layer 2)
// Wraps RPC layer and unwraps PlexusStreamItem to domain types

import type { RpcClient } from '../rpc';
import { collectOne } from '../rpc';

/** Typed client interface for spinner plugin */
export interface SpinnerClient {
  /**
   * Spin the fidget (requires scope spinner.spin)
   * @requiresCredential scopes: [spinner.spin], site: header:authorization
   */
  spin(): Promise<string>;
}

/** Typed client implementation for spinner plugin */
class SpinnerClientImpl implements SpinnerClient {
  private rpc: RpcClient;
  constructor(rpc: RpcClient) { this.rpc = rpc; }

  async spin(): Promise<string> {
    const stream = this.rpc.call('spinner.spin', {});
    return collectOne<string>(stream);
  }
}

/** Create a typed spinner client from an RPC client */
export function createSpinnerClient(rpc: RpcClient): SpinnerClient {
  return new SpinnerClientImpl(rpc);
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

/** Credential requirements for spinner methods (R-4). Surfacing only — the gate enforces server-side. */
export const SpinnerMethodAuth: { readonly [method: string]: MethodAuthMetadata } = {
  spin: { requiresCredential: { scopes: ['spinner.spin'], siteHint: 'header:authorization' } },
};