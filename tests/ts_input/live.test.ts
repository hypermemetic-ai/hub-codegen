// BIDIR-19 · the generated client's input half against a real Rust service
// (plexus-bench's `ingest` on BIDIR-17's branch: `items` takes Input<Item>,
// `bytes` Input<Bytes>), through a proxy that reads every WebSocket frame both
// ways — so what the service received, what credit it granted and what it
// acknowledged are counted on the wire, not by the client under test.
//
// PLEXUS_BENCH_PORT names the service's port (tests/bidir19_input_test.rs
// starts it). node --test.

import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import net from 'node:net';
import { createHash } from 'node:crypto';
import {
  Ingest,
  PlexusRpcClient,
  DisconnectedError,
  InvalidInputError,
  type DisconnectInfo,
} from '../index';

const UPSTREAM = Number(process.env.PLEXUS_BENCH_PORT);

// ─── the proxy: every frame both ways, per subscription ────────────────────

interface Sub {
  /** credit forwarded to the client */
  granted: number;
  /** input items the service received */
  received: number;
  /** frames the client sent past the credit it had been granted */
  pastCredit: number;
  /** the most items received and not yet acknowledged… as granted minus received, the least headroom seen */
  frames: Array<{ seq: number; binary: boolean; items: unknown[] }>;
  payloads: Buffer[];
  /** the last ack forwarded to the client */
  ackSeq: number | null;
  ended: boolean;
}

class FrameProxy {
  readonly server: net.Server;
  port = 0;
  hold = false;
  subs = new Map<number, Sub>();
  calls: string[] = [];
  unsubs = 0;
  textIn = 0;
  binaryIn = 0;
  private pairs: Array<[net.Socket, net.Socket]> = [];

  constructor(private upstream: number) {
    this.server = net.createServer((client) => this.accept(client));
  }

  listen(): Promise<void> {
    return new Promise((r) => this.server.listen(0, '127.0.0.1', () => { this.port = (this.server.address() as net.AddressInfo).port; r(); }));
  }

  close(): Promise<void> {
    this.cut();
    return new Promise((r) => this.server.close(() => r()));
  }

  /** Both sockets of every connection, dropped. */
  cut(): void {
    for (const [a, b] of this.pairs) { a.destroy(); b.destroy(); }
    this.pairs = [];
  }

  sub(id: number): Sub {
    let s = this.subs.get(id);
    if (!s) { s = { granted: 0, received: 0, pastCredit: 0, frames: [], payloads: [], ackSeq: null, ended: false }; this.subs.set(id, s); }
    return s;
  }

  /** Items in the frames through `seq`. */
  itemsThrough(s: Sub, seq: number | null): number {
    if (seq === null) return 0;
    return s.frames.filter((f) => f.seq <= seq).reduce((n, f) => n + f.items.length, 0);
  }

  private accept(client: net.Socket): void {
    const up = net.connect(this.upstream, '127.0.0.1');
    client.setNoDelay(true);
    up.setNoDelay(true);
    this.pairs.push([client, up]);
    client.on('error', () => {});
    up.on('error', () => {});
    client.on('close', () => up.destroy());
    up.on('close', () => client.destroy());
    const fromClient = reader((raw, op, data) => {
      if (op === 0x1) this.clientText(data.toString('utf8'));
      if (op === 0x2) this.clientBinary(data);
      up.write(raw);
    });
    const fromServer = reader((raw, op, data) => {
      if (this.hold) return; // held: never reaches the client
      if (op === 0x1) this.serverText(data.toString('utf8'));
      client.write(raw);
    });
    client.on('data', fromClient);
    up.on('data', fromServer);
  }

  private clientText(text: string): void {
    this.textIn += 1;
    const f = JSON.parse(text);
    const m: string = f.method ?? '';
    if (m.endsWith('.call')) this.calls.push(f.params.method);
    else if (m.endsWith('.call_unsub')) this.unsubs += 1;
    else if (m.endsWith('.input')) {
      const s = this.sub(f.params.subscription);
      s.frames.push({ seq: f.params.seq, binary: false, items: f.params.items });
      s.received += f.params.items.length;
      if (s.received > s.granted) s.pastCredit += 1;
    } else if (m.endsWith('.end')) this.sub(f.params.subscription).ended = true;
  }

  private clientBinary(data: Buffer): void {
    this.binaryIn += 1;
    assert.equal(data[0], 0x01, 'a binary input frame starts 0x01');
    const s = this.sub(Number(data.readBigUInt64BE(1)));
    s.frames.push({ seq: Number(data.readBigUInt64BE(9)), binary: true, items: [null] });
    s.payloads.push(Buffer.from(data.subarray(17)));
    s.received += 1;
    if (s.received > s.granted) s.pastCredit += 1;
  }

  private serverText(text: string): void {
    const f = JSON.parse(text);
    const r = f.params?.result;
    if (r?.type === 'credit') this.sub(f.params.subscription).granted += r.n;
    if (r?.type === 'ack') this.sub(f.params.subscription).ackSeq = r.seq;
  }
}

/**
 * A WebSocket byte stream as frames, after the HTTP upgrade (passed through
 * whole): `on(raw, opcode, payload)` per frame, `raw` its bytes as they came.
 */
function reader(on: (raw: Buffer, op: number, data: Buffer) => void): (chunk: Buffer) => void {
  let buf = Buffer.alloc(0);
  let upgraded = false;
  let parts: Buffer[] = [];
  let partOp = 0;
  return (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    if (!upgraded) {
      const end = buf.indexOf('\r\n\r\n');
      if (end < 0) return;
      on(buf.subarray(0, end + 4), -1, Buffer.alloc(0));
      buf = buf.subarray(end + 4);
      upgraded = true;
    }
    for (;;) {
      if (buf.length < 2) return;
      const fin = buf[0] & 0x80, op = buf[0] & 0x0f, masked = buf[1] & 0x80;
      let len = buf[1] & 0x7f, off = 2;
      if (len === 126) { if (buf.length < 4) return; len = buf.readUInt16BE(2); off = 4; }
      else if (len === 127) { if (buf.length < 10) return; len = Number(buf.readBigUInt64BE(2)); off = 10; }
      const mask = masked ? buf.subarray(off, off + 4) : null;
      if (masked) off += 4;
      if (buf.length < off + len) return;
      const raw = Buffer.from(buf.subarray(0, off + len));
      const data = Buffer.from(buf.subarray(off, off + len));
      if (mask) for (let i = 0; i < data.length; i++) data[i] ^= mask[i & 3];
      buf = buf.subarray(off + len);
      if (op === 0x0) {
        parts.push(data);
        if (fin) { const whole = Buffer.concat(parts); parts = []; on(raw, partOp, whole); } else on(raw, -1, Buffer.alloc(0));
      } else if ((op === 0x1 || op === 0x2) && !fin) {
        parts = [data]; partOp = op; on(raw, -1, Buffer.alloc(0));
      } else {
        on(raw, op, data);
      }
    }
  };
}

// ─── the client ────────────────────────────────────────────────────────────

let proxy: FrameProxy;

before(async () => {
  assert.ok(UPSTREAM > 0, 'PLEXUS_BENCH_PORT');
  proxy = new FrameProxy(UPSTREAM);
  await proxy.listen();
});

after(async () => { await proxy.close(); });

function client(): { rpc: PlexusRpcClient; ingest: Ingest.IngestClient } {
  const rpc = new PlexusRpcClient({ backend: 'perf5', url: `ws://127.0.0.1:${proxy.port}` });
  return { rpc, ingest: Ingest.createIngestClient(rpc) };
}

/** The subscription of the newest call, once the proxy has seen more than `before`. */
function newSub(before: number): Sub | undefined {
  if (proxy.subs.size <= before) return undefined;
  const subs = [...proxy.subs.values()];
  return subs[subs.length - 1];
}

const tick = (ms: number) => new Promise((r) => setTimeout(r, ms));

test('pull: 10,000 items, in order, every one, pulled only while credit allows', async () => {
  const { rpc, ingest } = client();
  const N = 10_000;
  const before = proxy.subs.size;
  let pulled = 0;
  let pulledPastCredit = 0;
  async function* items(): AsyncGenerator<Ingest.Item> {
    for (let i = 0; i < N; i++) {
      // the service has granted at least this many by now, on the wire
      const s = newSub(before);
      if (!s || pulled >= s.granted) pulledPastCredit += 1;
      pulled += 1;
      yield { at: 0, pad: String(i) };
    }
  }
  const ev = await ingest.items(items());
  assert.equal(ev.type, 'taken');
  assert.equal((ev as Ingest.IngestEventTaken).items, N);
  const s = newSub(before)!;
  assert.equal(pulledPastCredit, 0, 'pulled only within credit');
  assert.equal(s.pastCredit, 0);
  assert.equal(s.received, N);
  assert.ok(s.ended, 'end sent');
  const pads = s.frames.flatMap((f) => f.items.map((it) => (it as { pad: string }).pad));
  assert.deepEqual(pads, Array.from({ length: N }, (_, i) => String(i)), 'in order, all of them');
  assert.deepEqual(s.frames.map((f) => f.seq), s.frames.map((_, i) => i), 'seq contiguous from 0');
  assert.ok(s.frames.every((f) => !f.binary));
  rpc.disconnect();
});

test('push: 1 MiB in 640 B chunks as binary frames; never more than credit in flight, the queue grows instead', async () => {
  const { rpc, ingest } = client();
  const total = 1024 * 1024;
  const source = new Uint8Array(total);
  for (let i = 0; i < total; i++) source[i] = (i * 31 + 7) & 0xff;
  const before = proxy.subs.size;
  const call = ingest.openBytes({ pressureAt: 500 });
  const pressures: number[] = [];
  call.input.onPressure((p) => pressures.push(p.buffered));
  let disconnects = 0;
  call.onDisconnect(() => { disconnects += 1; });
  let chunks = 0;
  for (let at = 0; at < total; at += 640) { call.input.write(source.subarray(at, Math.min(at + 640, total))); chunks += 1; }
  const queued = call.input.buffered;
  call.input.end();
  const ev = await call.output;
  assert.equal(ev.type, 'received');
  const r = ev as Ingest.IngestEventReceived;
  assert.equal(r.items, chunks);
  assert.equal(r.bytes, total);
  assert.equal(queued, chunks, 'written before any credit came: all queued, nothing refused');
  assert.deepEqual(pressures, [500], 'onPressure once, as the queue passed its mark');
  const s = newSub(before)!;
  assert.equal(s.pastCredit, 0, 'never more than credit in flight (counted on the wire)');
  assert.equal(s.received, chunks);
  assert.ok(s.frames.every((f) => f.binary), 'every item a binary frame');
  assert.deepEqual(s.frames.map((f) => f.seq), s.frames.map((_, i) => i));
  assert.equal(createHash('sha256').update(Buffer.concat(s.payloads)).digest('hex'), createHash('sha256').update(source).digest('hex'), 'the bytes, in order');
  assert.equal(call.input.sent, chunks);
  assert.equal(call.input.acked, chunks);
  assert.equal(call.input.buffered, 0);
  rpc.disconnect();
  await tick(20);
  assert.equal(disconnects, 0, 'a call that ended is not disconnected by a later close');
});

test('a disconnect mid-push: onDisconnect and the output say what the service received and acknowledged; unsent() holds the rest', async () => {
  const { rpc, ingest } = client();
  const N = 20_000;
  const written = Array.from({ length: N }, (_, i): Ingest.Item => ({ at: 0, pad: String(i) }));
  const before = proxy.subs.size;
  const call = ingest.openItems();
  const said: DisconnectInfo[] = [];
  call.onDisconnect((d) => said.push(d));
  for (const it of written) call.input.write(it);
  // let some go through, then hold the service's answers back (no more credit, no more acks reach the client) and cut
  while ((newSub(before)?.received ?? 0) < 2_000) await tick(1);
  proxy.hold = true;
  await tick(200);
  const s = newSub(before)!;
  proxy.cut();
  await assert.rejects(call.output, (e: unknown) => {
    assert.ok(e instanceof DisconnectedError, `a DisconnectedError, not ${String(e)}`);
    assert.equal(e.sent, s.received);
    assert.equal(e.acked, proxy.itemsThrough(s, s.ackSeq));
    return true;
  });
  assert.equal(said.length, 1, 'onDisconnect once');
  const d = said[0];
  assert.equal(d.sent, s.received, 'sent = what the service received');
  assert.equal(d.acked, proxy.itemsThrough(s, s.ackSeq), 'acked = what the service acknowledged');
  assert.equal(d.ackSeq, s.ackSeq);
  assert.equal(d.frames, s.frames.length);
  assert.equal(d.unsent, N - d.sent);
  assert.equal(s.pastCredit, 0);
  assert.ok(d.sent > 0 && d.sent < N, `mid-push: ${d.sent} of ${N}`);
  assert.deepEqual(call.input.unsent(), written.slice(d.sent), 'unsent() holds the rest, in order');
  assert.deepEqual(call.input.unacked(), written.slice(d.acked, d.sent), 'unacked() what was sent and not acknowledged');
  // nothing was resent, and a write after it is kept, not sent
  call.input.write({ at: 0, pad: 'late' });
  assert.equal(call.input.buffered, N - d.sent + 1);
  // a handler added after is told too
  const late = await new Promise<DisconnectInfo>((r) => call.onDisconnect(r));
  assert.equal(late.sent, d.sent);
  proxy.hold = false;
  rpc.disconnect();
});

test('a typed refusal: an item that is not the input type is an InvalidInputError naming its frame', async () => {
  const { rpc, ingest } = client();
  const call = ingest.openItems();
  call.input.write({ at: 0, pad: 'fine' });
  call.input.write({ at: 'not a number', pad: 1 } as unknown as Ingest.Item);
  await assert.rejects(call.output, (e: unknown) => {
    assert.ok(e instanceof InvalidInputError, String(e));
    assert.equal(typeof e.seq, 'number');
    assert.equal(e.error_kind, 'invalid_input');
    return true;
  });
  rpc.disconnect();
});

test('an AbortSignal unsubscribes the call and its output throws the reason', async () => {
  const { rpc, ingest } = client();
  const ac = new AbortController();
  const unsubs = proxy.unsubs;
  let pulled = 0;
  async function* slow(): AsyncGenerator<Ingest.Item> {
    for (let i = 0; ; i++) { pulled += 1; if (i === 50) ac.abort(new Error('enough')); yield { at: 0, pad: String(i) }; await tick(1); }
  }
  await assert.rejects(ingest.items(slow(), { signal: ac.signal }), /enough/);
  await tick(50);
  assert.equal(proxy.unsubs, unsubs + 1, 'one call_unsub');
  const at = pulled;
  await tick(30);
  assert.equal(pulled, at, 'the source is pulled no more');
  rpc.disconnect();
});
