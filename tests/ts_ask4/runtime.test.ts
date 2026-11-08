// ASK-4 · a method that asks (ASK-1) on a mock WebSocket that plays the
// service (node --test): the proposal is an item like any other, the answer
// is the same method again with `confirm`, and nothing answers a `request`.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PlexusRpcClient, isNeedsInput, type NeedsInput, Tickets } from '../index';

const BACKEND = 'service';
const META = { provenance: ['service'], plexus_hash: 'x', timestamp: 1 };

interface Frame { jsonrpc: '2.0'; id?: number; method?: string; params?: any; result?: unknown }

class MockService {
  socket: MockSocket | null = null;
  sent: Frame[] = [];
  private nextSub = 100;
  /** What each call answers, in order: items for its subscription. */
  script: object[][] = [];
  calls(): Frame[] { return this.sent.filter((f) => f.method === `${BACKEND}.call`); }
  onFrame(f: Frame): void {
    this.sent.push(f);
    if (f.method === `${BACKEND}.call`) {
      const sub = this.nextSub++;
      this.reply({ jsonrpc: '2.0', id: f.id, result: sub });
      for (const item of this.script.shift() ?? []) {
        this.reply({ jsonrpc: '2.0', method: `${BACKEND}.call`, params: { subscription: sub, result: item } } as Frame);
      }
    } else if (f.method === `${BACKEND}.call_unsub`) {
      this.reply({ jsonrpc: '2.0', id: f.id, result: true });
    }
  }
  reply(f: Frame): void {
    setTimeout(() => this.socket?.onmessage?.({ data: JSON.stringify(f) }), 0);
  }
}

class MockSocket {
  static CONNECTING = 0; static OPEN = 1; static CLOSING = 2; static CLOSED = 3;
  readyState = MockSocket.CONNECTING;
  onopen: ((e: unknown) => void) | null = null;
  onerror: ((e: unknown) => void) | null = null;
  onclose: ((e: { code: number; reason: string }) => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  constructor(_url: string, private svc = currentService) {
    svc.socket = this;
    setTimeout(() => { this.readyState = MockSocket.OPEN; this.onopen?.({}); }, 0);
  }
  send(data: string): void { this.svc.onFrame(JSON.parse(data)); }
  close(): void { this.readyState = MockSocket.CLOSED; }
}
let currentService = new MockService();
(globalThis as any).WebSocket = MockSocket;

function setup() {
  const svc = new MockService();
  currentService = svc;
  const rpc = new PlexusRpcClient({ backend: BACKEND, url: 'ws://mock', preflight: false });
  return { svc, tickets: Tickets.createTicketsClient(rpc) };
}

const data = (method: string, content: object) => ({ type: 'data', metadata: META, content_type: method, content });
const DONE = { type: 'done', metadata: META };
const PROPOSAL = {
  type: 'needs_input', summary: 'archive KILN-2 (1 not done)', effects: [{ archive: 'KILN-2' }],
  fields: [{ name: 'children', options: ['reparent', 'delete'], default: 'reparent' }],
  token: 'v1.1794100000.abc', expires: '2026-11-07T12:00:00Z',
};

test('a streaming method that asks yields the proposal, then is called again with confirm', async () => {
  const { svc, tickets } = setup();
  svc.script.push([data('tickets.archive', PROPOSAL), DONE]);
  svc.script.push([data('tickets.archive', { type: 'archived', id: 'KILN-2' }), DONE]);
  const first: unknown[] = [];
  for await (const ev of tickets.archive('label stale')) first.push(ev);
  assert.equal(first.length, 1);
  const p = first[0];
  assert.ok(isNeedsInput(p));
  assert.deepEqual(p, PROPOSAL);
  // a first call sends no confirm at all
  assert.deepEqual(svc.calls()[0].params, { method: 'tickets.archive', params: { filter: 'label stale' } });

  const second: unknown[] = [];
  for await (const ev of tickets.archive('label stale', (p as NeedsInput).token)) second.push(ev);
  assert.deepEqual(second, [{ type: 'archived', id: 'KILN-2' }]);
  assert.ok(!isNeedsInput(second[0]));
  assert.deepEqual(svc.calls()[1].params, { method: 'tickets.archive', params: { filter: 'label stale', confirm: 'v1.1794100000.abc' } });
});

test('a one-shot method that asks answers the proposal; each field goes under its own name', async () => {
  const { svc, tickets } = setup();
  svc.script.push([data('tickets.delete', PROPOSAL), DONE]);
  svc.script.push([data('tickets.delete', { ids: ['KILN-2'] }), DONE]);
  const d = await tickets.delete(['KILN-2']);
  assert.ok(isNeedsInput(d));
  const children = d.fields[0].default as string;
  const done = await tickets.delete(['KILN-2'], children, d.token);
  assert.deepEqual(done, { ids: ['KILN-2'] });
  assert.deepEqual(svc.calls()[1].params.params, { ids: ['KILN-2'], children: 'reparent', confirm: 'v1.1794100000.abc' });
});

test('an old server\'s request item is neither answered nor yielded', async () => {
  const { svc, tickets } = setup();
  svc.script.push([
    { type: 'request', requestId: 'r-1', requestData: { type: 'confirm', message: 'go?' }, timeoutMs: 3000 },
    data('tickets.archive', { type: 'archived', id: 'KILN-2' }), DONE,
  ]);
  const got: unknown[] = [];
  for await (const ev of tickets.archive('label stale')) got.push(ev);
  assert.deepEqual(got, [{ type: 'archived', id: 'KILN-2' }]);
  assert.deepEqual(svc.sent.filter((f) => f.method === `${BACKEND}.respond`), []);
});

test('isNeedsInput is the type tag and nothing else', () => {
  assert.ok(isNeedsInput(PROPOSAL));
  for (const x of [null, undefined, 'needs_input', 1, {}, { type: 'archived' }, { kind: 'needs_input' }]) {
    assert.ok(!isNeedsInput(x), JSON.stringify(x));
  }
});
