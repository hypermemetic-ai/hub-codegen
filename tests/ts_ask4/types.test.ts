// ASK-4 · the types of a method that asks (ASK-1), checked by `tsc` (never
// run). Copied beside a package generated from tests/fixtures/ask4_asks_ir.json:
// tickets.archive streams and asks, tickets.delete answers once and asks,
// tickets.get asks nothing. Every `@ts-expect-error` below is an assertion:
// `tsc` fails if the line it covers is accepted.

import { expectTypeOf } from 'expect-type';
import {
  MethodStream, isNeedsInput,
  type ConfirmToken, type NeedsInput, type NeedsInputField, type RpcClient,
  Tickets,
} from '../index';

type Archive = Tickets.ArchiveEvent;
type Deleted = Tickets.Deleted;

export async function typeTests(rpc: RpcClient): Promise<void> {
  const tickets = Tickets.createTicketsClient(rpc);

  // ── the standard types ─────────────────────────────────────────────────
  expectTypeOf<ConfirmToken>().toEqualTypeOf<string>();
  expectTypeOf<NeedsInput['type']>().toEqualTypeOf<'needs_input'>();
  expectTypeOf<NeedsInput['token']>().toEqualTypeOf<ConfirmToken>();
  expectTypeOf<NeedsInput['fields']>().toEqualTypeOf<NeedsInputField[]>();
  expectTypeOf<NeedsInput['effects']>().toEqualTypeOf<unknown[]>();
  expectTypeOf<NeedsInputField['options']>().toEqualTypeOf<unknown[] | undefined>();
  expectTypeOf<NeedsInputField['help']>().toEqualTypeOf<string | undefined>();

  // ── a streaming method that asks: its items, or the proposal ────────────
  const s = tickets.archive('label stale');
  expectTypeOf(s).toEqualTypeOf<MethodStream<'tickets.archive', Archive | NeedsInput, Tickets.TicketsEvent>>();
  for await (const ev of s) {
    expectTypeOf(ev).toEqualTypeOf<Archive | NeedsInput>();
    if (isNeedsInput(ev)) {
      expectTypeOf(ev).toEqualTypeOf<NeedsInput>();
      // the answer: the same method, the same params, and the token
      tickets.archive('label stale', ev.token);
    } else {
      expectTypeOf(ev).toEqualTypeOf<Archive>();
    }
  }
  // @ts-expect-error — the item is not only the method's own
  expectTypeOf(tickets.archive('x')).toEqualTypeOf<MethodStream<'tickets.archive', Archive, Tickets.TicketsEvent>>();
  // @ts-expect-error — confirm is a token, a string
  tickets.archive('x', 42);

  // ── a one-shot method that asks ─────────────────────────────────────────
  const d = await tickets.delete(['KILN-2']);
  expectTypeOf(d).toEqualTypeOf<Deleted | NeedsInput>();
  if (isNeedsInput(d)) {
    for (const f of d.fields) expectTypeOf(f.name).toEqualTypeOf<string>();
    expectTypeOf(await tickets.delete(['KILN-2'], 'reparent', d.token)).toEqualTypeOf<Deleted | NeedsInput>();
  }

  // ── a method that asks nothing is as it was ─────────────────────────────
  expectTypeOf(await tickets.get('KILN-2')).toEqualTypeOf<Deleted>();
  // @ts-expect-error — and takes no confirm
  tickets.get('KILN-2', 'v1.1.x');

  // ── the aggregate carries the proposal for the methods that ask ─────────
  type Ev = Tickets.TicketsEvent;
  expectTypeOf<Extract<Ev, { method: 'archive' }>['event']>().toEqualTypeOf<Archive | NeedsInput>();
  expectTypeOf<Extract<Ev, { method: 'get' }>['event']>().toEqualTypeOf<Deleted>();
}
