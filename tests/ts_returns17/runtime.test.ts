// RETURNS-17 · the generated client's calls on a mock WebSocket that plays
// the hub and counts every frame (node --test): MethodStream's items,
// events and progress; RemoteCallError from an error item; TransportError
// from a lost socket; one reader; call_unsub on an early end; and watch over
// generated streams with no paths written.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  watch, PlexusRpcClient, PlexusError, Forbidden, ForbiddenError, RemoteCallError, TransportError, MethodStream,
  Alpha, Beta,
} from '../index';

const BACKEND = 'hub';
const META = { provenance: ['hub'], plexus_hash: 'x', timestamp: 1 };

interface Frame { jsonrpc: '2.0'; id?: number; method?: string; params?: any; result?: unknown }

class MockHub {
  socket: MockSocket | null = null;
  sent: Frame[] = [];
  private nextSub = 100;
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
    this.emit(method, { type: 'data', metadata: META, content_type: method, content }, n);
  }
  progress(method: string, message: string, percentage?: number): void {
    this.emit(method, { type: 'progress', metadata: META, message, ...(percentage === undefined ? {} : { percentage }) });
  }
  done(method: string, n = 0): void { this.emit(method, { type: 'done', metadata: META }, n); }
  error(method: string, fields: object, n = 0): void {
    this.emit(method, { type: 'error', metadata: META, recoverable: false, ...fields }, n);
  }
  drop(): void {
    setTimeout(() => { const s = this.socket!; s.readyState = MockSocket.CLOSED; s.onclose?.({ code: 1006, reason: '' }); }, 0);
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
  const rpc = new PlexusRpcClient({ backend: BACKEND, url: 'ws://mock', preflight: false });
  return { hub, rpc, alpha: Alpha.createAlphaClient(rpc), beta: Beta.createBetaClient(rpc) };
}

const tick = (ms = 5) => new Promise((r) => setTimeout(r, ms));
async function until(cond: () => boolean, what: string): Promise<void> {
  for (let i = 0; i < 400; i++) { if (cond()) return; await tick(2); }
  assert.fail(`timed out waiting for ${what}`);
}

test('a generated stream is a MethodStream that knows its path, and yields its items', async () => {
  const { hub, alpha } = setup();
  const s = alpha.a();
  assert.ok(s instanceof MethodStream);
  assert.equal(s.method, 'alpha.a');
  const got: unknown[] = [];
  const reading = (async () => { for await (const ev of s) got.push(ev); })();
  await until(() => hub.calls().length === 1, 'the call');
  assert.equal(hub.calls()[0].params.method, 'alpha.a');
  hub.data('alpha.a', { n: 1 });
  hub.progress('alpha.a', 'half');
  hub.data('alpha.a', { n: 2 });
  hub.done('alpha.a');
  await reading;
  assert.deepEqual(got, [{ n: 1 }, { n: 2 }], 'progress dropped');
  assert.equal(hub.unsubs().length, 0, 'it ended at the server: nothing to unsubscribe');
});

test('.events() tags each event with the method', async () => {
  const { hub, beta } = setup();
  const got: unknown[] = [];
  const reading = (async () => { for await (const ev of beta.b().events()) got.push(ev); })();
  await until(() => hub.calls().length === 1, 'the call');
  hub.data('beta.b', { text: 'x' });
  hub.done('beta.b');
  await reading;
  assert.deepEqual(got, [{ method: 'beta.b', event: { text: 'x' } }]);
});

test('.withProgress() yields items and progress in arrival order', async () => {
  const { hub, alpha } = setup();
  const got: unknown[] = [];
  const reading = (async () => { for await (const f of alpha.a().withProgress()) got.push(f); })();
  await until(() => hub.calls().length === 1, 'the call');
  hub.progress('alpha.a', 'starting');
  hub.data('alpha.a', { n: 1 });
  hub.progress('alpha.a', 'half', 50);
  hub.done('alpha.a');
  await reading;
  assert.deepEqual(got, [
    { progress: { message: 'starting' } },
    { item: { n: 1 } },
    { progress: { message: 'half', percentage: 50 } },
  ]);
});

test('an error item throws RemoteCallError, typed from the item — still the AUTH-9 class', async () => {
  const { hub, alpha } = setup();
  const got: unknown[] = [];
  const reading = (async () => { for await (const ev of alpha.a()) got.push(ev); })();
  await until(() => hub.calls().length === 1, 'the call');
  hub.data('alpha.a', { n: 1 });
  hub.error('alpha.a', { code: '-32003', error_kind: 'forbidden', grant: 'alpha.a', scope: '*', roles: ['editor', 'admin'], held_on: [], message: 'needs alpha.a' });
  const e = await reading.then(() => assert.fail('it must throw'), (x: unknown) => x);
  assert.deepEqual(got, [{ n: 1 }], 'the item before the error arrived');
  assert.ok(e instanceof RemoteCallError, 'a RemoteCallError');
  assert.ok(e instanceof PlexusError && e instanceof ForbiddenError && e instanceof Forbidden, 'and the class AUTH-9 throws');
  assert.equal(e.method, 'alpha.a');
  assert.equal(e.error_kind, 'forbidden');
  assert.equal(e.grant, 'alpha.a');
  assert.equal(e.scope, '*');
  assert.deepEqual(e.roles, ['editor', 'admin']);
  assert.deepEqual(e.held_on, []);
  assert.equal(e.code, '-32003');
  assert.deepEqual(e.error, { code: '-32003', error_kind: 'forbidden', grant: 'alpha.a', scope: '*', roles: ['editor', 'admin'], held_on: [], message: 'needs alpha.a', recoverable: false });
  assert.equal(hub.unsubs().length, 0, 'an error item ends the call at the server');
});

test('a one-item call throws RemoteCallError too, its kind read from the item', async () => {
  const { hub, alpha } = setup();
  const p = alpha.get('x');
  await until(() => hub.calls().length === 1, 'the call');
  hub.error('alpha.get', { code: 'too_many_calls', error_kind: 'too_many_calls', max: 8, message: 'too many', recoverable: true });
  const e = await p.then(() => assert.fail('it must throw'), (x: unknown) => x);
  assert.ok(e instanceof RemoteCallError && e instanceof PlexusError);
  assert.equal(e.method, undefined);
  assert.equal(e.error.error_kind, 'too_many_calls');
  if (e.error.error_kind === 'too_many_calls') assert.equal(e.error.max, 8);
  assert.equal(e.recoverable, true);
});

test('a socket lost mid-stream throws TransportError, not a quiet end', async () => {
  const { hub, alpha } = setup();
  const got: unknown[] = [];
  const reading = (async () => { for await (const ev of alpha.a()) got.push(ev); })();
  await until(() => hub.calls().length === 1, 'the call');
  hub.data('alpha.a', { n: 1 });
  await until(() => got.length === 1, 'the item');
  hub.drop();
  const e = await reading.then(() => assert.fail('it must throw'), (x: unknown) => x);
  assert.ok(e instanceof TransportError, String(e));
  assert.ok(!(e instanceof RemoteCallError), 'never a remote error');
  assert.match((e as Error).message, /alpha\.a/);
});

test('read once: after .events(), for await and .withProgress() are refused', async () => {
  const { alpha } = setup();
  const s = alpha.a();
  s.events();
  assert.throws(() => s.events(), /already being read/);
  assert.throws(() => s.withProgress(), /already being read/);
  assert.throws(() => s.next(), /already being read/);
});

test('breaking out of for await sends one call_unsub', async () => {
  const { hub, alpha } = setup();
  const reading = (async () => { for await (const _ of alpha.a()) break; })();
  await until(() => hub.calls().length === 1, 'the call');
  hub.data('alpha.a', { n: 1 });
  await reading;
  await until(() => hub.unsubs().length === 1, 'the unsubscribe');
  assert.deepEqual(hub.unsubs()[0].params, [hub.subOf('alpha.a')]);
});

test('return() before reading makes no call', async () => {
  const { hub, alpha } = setup();
  const s = alpha.a();
  assert.deepEqual(await s.return(), { value: undefined, done: true });
  await tick(20);
  assert.equal(hub.calls().length, 0);
});

test('watch over generated streams: no paths written; close unsubscribes each, parked or not', async () => {
  const { hub, alpha, beta } = setup();
  const s = watch(alpha.a(), beta.b());
  assert.deepEqual(s.sources, ['alpha.a', 'beta.b']);
  const first = s.next();
  await until(() => hub.calls().length === 2, 'two subscriptions');
  hub.data('beta.b', { text: 'b1' });
  assert.deepEqual((await first).value, { source: 'beta.b', event: { text: 'b1' } });
  const parked = s.next();
  await tick(20);
  await s.close();
  assert.deepEqual(await parked, { value: undefined, done: true });
  await until(() => hub.unsubs().length === 2, 'two unsubscribes');
  assert.deepEqual(hub.unsubs().map((f) => f.params[0]).sort(), [hub.subOf('alpha.a'), hub.subOf('beta.b')].sort());
});

test('a RemoteCallError built directly is one; a plain PlexusError is not', () => {
  const e = new RemoteCallError('alpha.a', { code: '-32602', error_kind: 'invalid_params', message: 'Invalid params: id', recoverable: false });
  assert.ok(e instanceof RemoteCallError && e instanceof PlexusError);
  assert.equal(e.name, 'RemoteCallError');
  assert.equal(e.error_kind, 'invalid_params');
  assert.equal(e.grant, undefined);
  assert.ok(!(new PlexusError('x') instanceof RemoteCallError));
  assert.ok(!(new TransportError('x') instanceof RemoteCallError));
});
