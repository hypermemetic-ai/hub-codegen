// RETURNS-17 · the types of the generated client's calls, checked by `tsc`
// (never run). Copied beside a package generated from
// tests/fixtures/returns17_ir.json: alpha.a, beta.b and gamma.c stream three
// unrelated event types; alpha.get answers one. Every `@ts-expect-error`
// below is an assertion: `tsc` fails if the line it covers is accepted.

import { expectTypeOf } from 'expect-type';
import {
  watch, MethodStream, RemoteCallError, PlexusError, TransportError, isOk, isErr,
  type Merged, type Tagged, type MethodEvent, type WithProgress, type Progress,
  type RemoteError, type RemoteErrorKind, type Result, type RpcClient,
  Alpha, Beta, Gamma,
} from '../index';

type A = Alpha.AlphaEvent;
type B = Beta.BetaEvent;
type C = Gamma.GammaEvent;

export async function typeTests(rpc: RpcClient): Promise<void> {
  const alpha = Alpha.createAlphaClient(rpc);
  const beta = Beta.createBetaClient(rpc);
  const gamma = Gamma.createGammaClient(rpc);

  // ── a generated stream carries its path ─────────────────────────────────
  const a = alpha.a();
  expectTypeOf(a).toEqualTypeOf<MethodStream<'alpha.a', A>>();
  expectTypeOf(a.method).toEqualTypeOf<'alpha.a'>();
  // @ts-expect-error — the path is a literal, never widened to string
  expectTypeOf(a).toEqualTypeOf<MethodStream<string, A>>();
  // @ts-expect-error — and the event is the method's own
  expectTypeOf(a).toEqualTypeOf<MethodStream<'alpha.a', A | B>>();
  for await (const ev of a) expectTypeOf(ev).toEqualTypeOf<A>();
  for await (const ev of beta.b().events()) expectTypeOf(ev).toEqualTypeOf<MethodEvent<'beta.b', B>>();
  for await (const f of gamma.c().withProgress()) {
    expectTypeOf(f).toEqualTypeOf<WithProgress<C>>();
    if ('progress' in f) expectTypeOf(f.progress).toEqualTypeOf<Progress>();
    else expectTypeOf(f.item).toEqualTypeOf<C>();
  }
  // a one-item call is still a promise of its item
  expectTypeOf(alpha.get('x')).toEqualTypeOf<Promise<A>>();

  // ── watch: no paths written ─────────────────────────────────────────────
  const s = watch(alpha.a(), beta.b(), gamma.c());
  expectTypeOf(s).toEqualTypeOf<Merged<Tagged<'alpha.a', A> | Tagged<'beta.b', B> | Tagged<'gamma.c', C>>>();
  // @ts-expect-error — never widened to the union under any source
  expectTypeOf(s).toEqualTypeOf<Merged<Tagged<string, A | B | C>>>();
  // mixed with a pair
  async function* plain(): AsyncGenerator<{ readonly env: string }> {}
  expectTypeOf(watch(alpha.a(), ['deployments.watch', plain()])).toEqualTypeOf<
    Merged<Tagged<'alpha.a', A> | Tagged<'deployments.watch', { readonly env: string }>>
  >();
}

// ── RemoteCallError: the wire's error item, typed ─────────────────────────
export function remoteCallErrorTests(e: unknown): void {
  if (e instanceof RemoteCallError) {
    expectTypeOf(e).toMatchTypeOf<PlexusError>();
    expectTypeOf(e.method).toEqualTypeOf<string | undefined>();
    expectTypeOf(e.error).toEqualTypeOf<RemoteError>();
    expectTypeOf(e.error_kind).toEqualTypeOf<RemoteErrorKind | undefined>();
    expectTypeOf(e.grant).toEqualTypeOf<string | undefined>();
    expectTypeOf(e.scope).toEqualTypeOf<string | undefined>();
    expectTypeOf(e.roles).toEqualTypeOf<readonly string[] | undefined>();
    expectTypeOf(e.held_on).toEqualTypeOf<readonly string[] | undefined>();
    const err = e.error;
    if (err.error_kind === 'forbidden') {
      expectTypeOf(err.grant).toEqualTypeOf<string | undefined>();
      expectTypeOf(err.held_on).toEqualTypeOf<readonly string[] | undefined>();
      expectTypeOf(err.reason).toEqualTypeOf<'missing_scope' | 'tenant_boundary' | 'not_accepted' | undefined>();
    } else if (err.error_kind === 'too_many_calls') {
      expectTypeOf(err.max).toEqualTypeOf<number>();
      // @ts-expect-error — a too_many_calls refusal names no grant
      err.grant;
    } else if (err.error_kind === undefined) {
      expectTypeOf(err.code).toEqualTypeOf<string>();
    }
    // @ts-expect-error — the wire's message is for people; there is no parsed scope
    e.missingScope;
  }
  if (e instanceof TransportError) {
    // @ts-expect-error — a transport failure never crossed the wire: no error item
    e.error;
  }
  expectTypeOf<RemoteErrorKind>().toEqualTypeOf<'invalid_params' | 'unauthenticated' | 'forbidden' | 'too_many_calls'>();
  // @ts-expect-error — a kind the wire does not send
  const k: RemoteErrorKind = 'not_found';
  void k;
}

// ── Result items: the wire's shape ────────────────────────────────────────
export function resultTests(r: Result<A, { type: 'not_found'; id: string }>): void {
  expectTypeOf<Result<number, string>>().toEqualTypeOf<{ ok: number } | { err: string }>();
  if (isOk(r)) expectTypeOf(r.ok).toEqualTypeOf<A>();
  else expectTypeOf(r.err).toEqualTypeOf<{ type: 'not_found'; id: string }>();
  if (isErr(r)) expectTypeOf(r.err.id).toEqualTypeOf<string>();
  // @ts-expect-error — narrowed to neither half, it has no `ok`
  r.ok;
}
