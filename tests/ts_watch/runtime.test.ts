// RETURNS-38 · the behaviour of watch(a, b, c), through the generated
// transport and clients, on a mock WebSocket that plays the hub and counts
// every frame (node --test).
//
// The behaviour table: arrival order; it ends when every source has; a
// failing source is an item and the others go on; `errors: 'stop'` ends it;
// close / break / abort send exactly one `call_unsub` per subscription
// still open; and the frames of watch(a, b, c) are the frames of three
// hand-made streams.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { watch, watchWith, PlexusRpcClient, PlexusError, Alpha, Beta, Gamma, type MethodSource } from '../index';

const BACKEND = 'hub';

interface Frame { jsonrpc: '2.0'; id?: number; method?: string; params?: any; result?: unknown }

/** The hub behind the mock socket: acknowledges each call with a subscription, and lets a test speak on it. */
class MockHub {
  socket: MockSocket | null = null;
  sent: Frame[] = [];
  private nextSub = 100;
  /** method path → its subscriptions, in call order */
  subs = new Map<string, number[]>();

  calls(): Frame[] { return this.sent.filter((f) => f.method === `${BACKEND}.call`); }
  unsubs(): Frame[] { return this.sent.filter((f) => f.method === `${BACKEND}.call_unsub`); }
  subOf(method: string, n = 0): number {
    const s = this.subs.get(method)?.[n];
    assert.notEqual(s, undefined, `no subscription for ${method}`);
    return s!;
  }

  onFrame(f: Frame): void {
    this.sent.push(f);
    if (f.method === `${BACKEND}.call`) {
      const sub = this.nextSub++;
      const m: string = f.params.method;
      this.subs.set(m, [...(this.subs.get(m) ?? []), sub]);
      this.reply({ jsonrpc: '2.0', id: f.id, result: sub });
    } else if (f.method === `${BACKEND}.call_unsub`) {
      this.reply({ jsonrpc: '2.0', id: f.id, result: true });
    }
  }

  reply(f: Frame): void {
    setTimeout(() => this.socket?.onmessage?.({ data: JSON.stringify(f) }), 0);
  }

  emit(method: string, item: object, n = 0): void {
    this.reply({ jsonrpc: '2.0', method: `${BACKEND}.call`, params: { subscription: this.subOf(method, n), result: item } } as Frame);
  }
  data(method: string, content: object, n = 0): void {
    this.emit(method, { type: 'data', metadata: { provenance: [], plexus_hash: 'x', timestamp: 1 }, content_type: method, content }, n);
  }
  done(method: string, n = 0): void { this.emit(method, { type: 'done', metadata: { provenance: [], plexus_hash: 'x', timestamp: 1 } }, n); }
  error(method: string, message: string, n = 0): void {
    this.emit(method, { type: 'error', metadata: { provenance: [], plexus_hash: 'x', timestamp: 1 }, message, code: '-32003', recoverable: false }, n);
  }
}

class MockSocket {
  static CONNECTING = 0; static OPEN = 1; static CLOSING = 2; static CLOSED = 3;
  readyState = MockSocket.CONNECTING;
  onopen: ((e: unknown) => void) | null = null;
  onerror: ((e: unknown) => void) | null = null;
  onclose: ((e: { code: number; reason: string }) => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  constructor(_url: string, private hub = currentHub) {
    hub.socket = this;
    setTimeout(() => { this.readyState = MockSocket.OPEN; this.onopen?.({}); }, 0);
  }
  send(data: string): void { this.hub.onFrame(JSON.parse(data)); }
  close(): void { this.readyState = MockSocket.CLOSED; }
}
let currentHub = new MockHub();
(globalThis as any).WebSocket = MockSocket;

function setup() {
  const hub = new MockHub();
  currentHub = hub;
  const rpc = new PlexusRpcClient({ backend: BACKEND, url: 'ws://mock' });
  return {
    hub,
    rpc,
    alpha: Alpha.createAlphaClient(rpc),
    beta: Beta.createBetaClient(rpc),
    gamma: Gamma.createGammaClient(rpc),
  };
}

const tick = (ms = 5) => new Promise((r) => setTimeout(r, ms));
async function until(cond: () => boolean, what: string): Promise<void> {
  for (let i = 0; i < 400; i++) { if (cond()) return; await tick(2); }
  assert.fail(`timed out waiting for ${what}`);
}
/** A promise's state, without waiting for it. */
async function settled<T>(p: Promise<T>): Promise<boolean> {
  const marker = Symbol();
  return (await Promise.race([p, tick(20).then(() => marker)])) !== marker;
}

test('arrival order, across sources', async () => {
  const { hub, alpha, beta, gamma } = setup();
  const s = watch(['alpha.a', alpha.a()], ['beta.b', beta.b()], ['gamma.c', gamma.c()]);
  const first = s.next();
  await until(() => hub.calls().length === 3, 'three subscriptions');
  assert.equal(await settled(first), false, 'nothing yet');

  hub.data('beta.b', { text: 'b1' });
  assert.deepEqual((await first).value, { source: 'beta.b', event: { text: 'b1' } });

  hub.data('gamma.c', { flag: true });
  hub.data('alpha.a', { n: 1 });
  hub.data('beta.b', { text: 'b2' });
  const got = [(await s.next()).value, (await s.next()).value, (await s.next()).value];
  assert.deepEqual(got, [
    { source: 'gamma.c', event: { flag: true } },
    { source: 'alpha.a', event: { n: 1 } },
    { source: 'beta.b', event: { text: 'b2' } },
  ]);
  await s.close();
});

test('one source ending leaves the merge open; it ends when all have', async () => {
  const { hub, alpha, beta, gamma } = setup();
  const s = watch(['alpha.a', alpha.a()], ['beta.b', beta.b()], ['gamma.c', gamma.c()]);
  const next = s.next();
  await until(() => hub.calls().length === 3, 'three subscriptions');
  hub.done('alpha.a');
  assert.equal(await settled(next), false, 'still open after alpha ended');
  hub.data('gamma.c', { flag: false });
  assert.deepEqual((await next).value, { source: 'gamma.c', event: { flag: false } });
  hub.done('beta.b');
  hub.done('gamma.c');
  assert.deepEqual(await s.next(), { value: undefined, done: true });
  assert.deepEqual(await s.next(), { value: undefined, done: true });
  await tick();
  assert.equal(hub.unsubs().length, 0, 'every subscription ended at the server: nothing to unsubscribe');
});

test('a failing source is an item naming it; the others go on', async () => {
  const { hub, alpha, beta, gamma } = setup();
  const s = watch(['alpha.a', alpha.a()], ['beta.b', beta.b()], ['gamma.c', gamma.c()]);
  const next = s.next();
  await until(() => hub.calls().length === 3, 'three subscriptions');
  hub.error('beta.b', 'forbidden');
  const failure = (await next).value;
  assert.ok(failure && 'error' in failure);
  assert.equal(failure.source, 'beta.b');
  assert.ok(failure.error instanceof PlexusError);
  assert.equal(failure.error.message, 'forbidden');

  hub.data('alpha.a', { n: 2 });
  assert.deepEqual((await s.next()).value, { source: 'alpha.a', event: { n: 2 } });
  await s.close();
  await until(() => hub.unsubs().length === 2, 'two unsubscribes');
  await tick();
  assert.deepEqual(hub.unsubs().map((f) => f.params[0]).sort(), [hub.subOf('alpha.a'), hub.subOf('gamma.c')].sort());
});

test("errors: 'stop' — the failure is the last item, the rest unsubscribed at once", async () => {
  const { hub, alpha, beta, gamma } = setup();
  const s = watchWith({ errors: 'stop' }, ['alpha.a', alpha.a()], ['beta.b', beta.b()], ['gamma.c', gamma.c()]);
  const next = s.next();
  await until(() => hub.calls().length === 3, 'three subscriptions');
  hub.error('gamma.c', 'boom');
  const failure = (await next).value;
  assert.ok(failure && 'error' in failure && failure.source === 'gamma.c');
  // alpha and beta are parked waiting: they are unsubscribed now, not on close.
  await until(() => hub.unsubs().length === 2, 'two unsubscribes');
  assert.deepEqual(await s.next(), { value: undefined, done: true });
  await tick();
  assert.equal(hub.unsubs().length, 2, 'once each');
});

for (const how of ['close()', 'break', 'abort'] as const) {
  test(`${how} with three parked subscriptions sends exactly three call_unsub`, async () => {
    const { hub, alpha, beta, gamma } = setup();
    const ac = new AbortController();
    const s = watchWith({ signal: ac.signal }, ['alpha.a', alpha.a()], ['beta.b', beta.b()], ['gamma.c', gamma.c()]);
    if (how === 'break') {
      const consumed = (async () => { for await (const _ of s) break; })();
      await until(() => hub.calls().length === 3, 'three subscriptions');
      hub.data('alpha.a', { n: 1 });
      await consumed;
    } else {
      const parked = s.next();
      await until(() => hub.calls().length === 3, 'three subscriptions');
      assert.equal(await settled(parked), false, 'every source is parked');
      if (how === 'close()') await s.close(); else ac.abort();
      assert.deepEqual(await parked, { value: undefined, done: true });
    }
    await until(() => hub.unsubs().length >= 3, 'three unsubscribes');
    await tick(30);
    const unsubs = hub.unsubs();
    assert.equal(unsubs.length, 3, 'exactly one each');
    assert.deepEqual(unsubs.map((f) => f.params[0]).sort(), ['alpha.a', 'beta.b', 'gamma.c'].map((m) => hub.subOf(m)).sort());
  });
}

test('wire: the frames of watch(a, b, c) are those of three hand-made streams', async () => {
  const shape = (hub: MockHub) => {
    const methodOf = new Map<number, string>();
    for (const [m, subs] of hub.subs) for (const s of subs) methodOf.set(s, m);
    return hub.sent.map((f) => f.method === `${BACKEND}.call` ? `call ${f.params.method}` : `${f.method} ${methodOf.get(f.params[0])}`).sort();
  };

  // By hand: three streams, one item each, then let go.
  const byHand = setup();
  const gens = [byHand.alpha.a(), byHand.beta.b(), byHand.gamma.c()];
  const firsts = gens.map((g) => g.next());
  await until(() => byHand.hub.calls().length === 3, 'three subscriptions');
  byHand.hub.data('alpha.a', { n: 1 }); byHand.hub.data('beta.b', { text: 'x' }); byHand.hub.data('gamma.c', { flag: true });
  await Promise.all(firsts);
  for (const g of gens) await g.return(undefined);
  await until(() => byHand.hub.unsubs().length === 3, 'three unsubscribes by hand');

  // Merged.
  const merged = setup();
  const s = watch(['alpha.a', merged.alpha.a()], ['beta.b', merged.beta.b()], ['gamma.c', merged.gamma.c()]);
  const first = s.next();
  await until(() => merged.hub.calls().length === 3, 'three subscriptions');
  merged.hub.data('alpha.a', { n: 1 }); merged.hub.data('beta.b', { text: 'x' }); merged.hub.data('gamma.c', { flag: true });
  await first; await s.next(); await s.next();
  await s.close();
  await until(() => merged.hub.unsubs().length === 3, 'three unsubscribes merged');
  await tick(30);

  assert.equal(merged.hub.sent.length, 6, '3 calls + 3 unsubscribes, no more');
  assert.deepEqual(shape(merged.hub), shape(byHand.hub));
});

test('one argument is tagged and ends with its stream', async () => {
  const { hub, alpha } = setup();
  const s = watch(['alpha.a', alpha.a()]);
  const next = s.next();
  await until(() => hub.calls().length === 1, 'a subscription');
  hub.data('alpha.a', { n: 7 });
  hub.done('alpha.a');
  assert.deepEqual((await next).value, { source: 'alpha.a', event: { n: 7 } });
  assert.equal((await s.next()).done, true);
});

test('a stream already being watched is refused at the call', () => {
  const { alpha } = setup();
  const g = alpha.a();
  assert.throws(() => watch(['alpha.a', g], ['again', g]), /already being watched/);
  const h = alpha.a();
  const s = watch(['alpha.a', h]);
  assert.throws(() => watch(['alpha.a', h]), /already being watched/);
  void s.close();
  assert.throws(() => (watch as any)(), /at least one/);
  assert.throws(() => (watch as any)(42), /neither a MethodStream nor/);
});

test('a MethodStream source is tagged by its own path', async () => {
  async function* events() { yield { type: 'posted', message: 1 } as const; yield { type: 'posted', message: 2 } as const; }
  const chats: MethodSource<'chats.watch', { type: 'posted'; message: number }> = Object.assign(events(), { method: 'chats.watch' as const });
  const items = [];
  for await (const it of watch(chats)) items.push(it);
  assert.deepEqual(items, [
    { source: 'chats.watch', event: { type: 'posted', message: 1 } },
    { source: 'chats.watch', event: { type: 'posted', message: 2 } },
  ]);
});

test('a break out of a bare stream unsubscribes too (the transport, RETURNS-38)', async () => {
  const { hub, alpha } = setup();
  const consumed = (async () => { for await (const _ of alpha.a()) break; })();
  await until(() => hub.calls().length === 1, 'a subscription');
  hub.data('alpha.a', { n: 1 });
  await consumed;
  await until(() => hub.unsubs().length === 1, 'an unsubscribe');
  assert.equal(hub.unsubs()[0].params[0], hub.subOf('alpha.a'));
});
