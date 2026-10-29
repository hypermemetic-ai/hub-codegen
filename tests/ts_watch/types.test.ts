// RETURNS-38 · the type of watch(a, b, c), checked by `tsc` (never run).
//
// Copied beside a package generated from the three-namespace fixture of
// tests/returns38_watch_test.rs: alpha.a, beta.b and gamma.c stream three
// unrelated event types. Every `@ts-expect-error` below is an assertion:
// `tsc` fails if the line it covers is accepted.

import { expectTypeOf } from 'expect-type';
import {
  watch, watchWith,
  type Merged, type MethodStream, type RpcClient, type Tagged, type WatchFailure,
  Alpha, Beta, Gamma,
} from '../index';

type A = Alpha.AlphaEvent;
type B = Beta.BetaEvent;
type C = Gamma.GammaEvent;
type ItemOf<M> = M extends AsyncIterable<infer I> ? I : never;

export async function typeTests(rpc: RpcClient): Promise<void> {
  const alpha = Alpha.createAlphaClient(rpc);
  const beta = Beta.createBetaClient(rpc);
  const gamma = Gamma.createGammaClient(rpc);

  // ── three streams of different methods: the exact union ─────────────────
  const s = watch(['alpha.a', alpha.a()], ['beta.b', beta.b()], ['gamma.c', gamma.c()]);
  expectTypeOf(s).toEqualTypeOf<Merged<Tagged<'alpha.a', A> | Tagged<'beta.b', B> | Tagged<'gamma.c', C>>>();
  expectTypeOf<ItemOf<typeof s>>().toEqualTypeOf<
    | Tagged<'alpha.a', A> | Tagged<'beta.b', B> | Tagged<'gamma.c', C>
    | WatchFailure<'alpha.a' | 'beta.b' | 'gamma.c'>
  >();
  expectTypeOf(s.sources).toEqualTypeOf<readonly ('alpha.a' | 'beta.b' | 'gamma.c')[]>();

  // ── widening to an activation-wide event is a type error ────────────────
  type Wide = A | B | C;
  // @ts-expect-error — the item is not the wide event under any source
  expectTypeOf(s).toEqualTypeOf<Merged<Tagged<string, Wide>>>();
  // @ts-expect-error — nor the wide event under the right sources
  expectTypeOf(s).toEqualTypeOf<Merged<Tagged<'alpha.a' | 'beta.b' | 'gamma.c', Wide>>>();
  // @ts-expect-error — nor a bare union with the sources dropped
  expectTypeOf<ItemOf<typeof s>>().toEqualTypeOf<A | B | C>();

  // ── narrowing by source gives each method's own event ───────────────────
  for await (const it of s) {
    if ('error' in it) {
      expectTypeOf(it.error).toEqualTypeOf<Error>();
      expectTypeOf(it.source).toEqualTypeOf<'alpha.a' | 'beta.b' | 'gamma.c'>();
      continue;
    }
    if (it.source === 'beta.b') expectTypeOf(it.event).toEqualTypeOf<B>();
    else if (it.source === 'alpha.a') expectTypeOf(it.event).toEqualTypeOf<A>();
    else expectTypeOf(it.event).toEqualTypeOf<C>();
  }

  // ── no argument is a type error ──────────────────────────────────────────
  // @ts-expect-error — at least one stream
  watch();
  // @ts-expect-error — at least one stream, with options too
  watchWith({ errors: 'stop' });

  // ── one argument: its own type ───────────────────────────────────────────
  expectTypeOf(watch(['alpha.a', alpha.a()])).toEqualTypeOf<Merged<Tagged<'alpha.a', A>>>();

  // ── what is not a source is refused ─────────────────────────────────────
  // @ts-expect-error — not a stream
  watch(['alpha.a', 42]);
  // @ts-expect-error — a path is a string
  watch([1, alpha.a()]);
  // RETURNS-17: a generated stream is a MethodStream — it brings its path
  expectTypeOf(watch(alpha.a())).toEqualTypeOf<Merged<Tagged<'alpha.a', A>>>();
  async function* bare(): AsyncGenerator<A> {}
  // @ts-expect-error — a bare generator has no path to tag it with
  watch(bare());

  // ── options do not change the type ──────────────────────────────────────
  const stopping = watchWith({ errors: 'stop' }, ['alpha.a', alpha.a()], ['gamma.c', gamma.c()]);
  expectTypeOf(stopping).toEqualTypeOf<Merged<Tagged<'alpha.a', A> | Tagged<'gamma.c', C>>>();
  // @ts-expect-error — 'stop' or 'continue'
  watchWith({ errors: 'ignore' }, ['alpha.a', alpha.a()]);

  // ── the same method twice is two sources of one type ────────────────────
  expectTypeOf(watch(['alpha.a', alpha.a()], ['alpha.a', alpha.a()])).toEqualTypeOf<Merged<Tagged<'alpha.a', A>>>();
}

// ── MethodStream: a stream that carries its path needs none written ───────
interface TicketsWatchEvent { readonly type: 'updated'; readonly id: string }
interface ChatsWatchEvent { readonly type: 'posted'; readonly message: number }

export function methodStreamTests(
  tickets: MethodStream<'tickets.watch', TicketsWatchEvent>,
  chats: MethodStream<'chats.watch', ChatsWatchEvent>,
  deployments: AsyncGenerator<{ readonly env: string }>,
): void {
  expectTypeOf(watch(tickets, chats)).toEqualTypeOf<
    Merged<Tagged<'tickets.watch', TicketsWatchEvent> | Tagged<'chats.watch', ChatsWatchEvent>>
  >();
  // Mixed with a pair.
  expectTypeOf(watch(tickets, ['deployments.watch', deployments])).toEqualTypeOf<
    Merged<Tagged<'tickets.watch', TicketsWatchEvent> | Tagged<'deployments.watch', { readonly env: string }>>
  >();
  // @ts-expect-error — the path is the stream's, never widened to string
  expectTypeOf(watch(tickets)).toEqualTypeOf<Merged<Tagged<string, TicketsWatchEvent>>>();
}
