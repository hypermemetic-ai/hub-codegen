// BIDIR-19 review · the input half when its call ends without a done or an
// error of its own — refused at open, or let go by its reader — and an abort
// before the call starts; on a fake duplex RpcClient that records every frame
// (node --test, no service).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  Ingest,
  PlexusRpcClient,
  pullInput,
  pushInput,
  type InputHalf,
  type PlexusStreamItem,
} from '../index';

const tick = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** A duplex RpcClient: grants `credit`, then plays `script` as the call. */
class FakeRpc {
  readonly carriesInput = true as const;
  frames: string[] = [];
  started = 0;
  ended = 0;
  constructor(private credit: number, private script: 'refuse' | 'data-then-wait') {}

  call(_method: string, _params?: unknown, input?: InputHalf<unknown>): AsyncGenerator<PlexusStreamItem> {
    const self = this;
    return (async function* () {
      self.started += 1;
      try {
        if (self.script === 'refuse') throw new Error('refused at open');
        input!.open({ backend: 'svc', subscription: 7, text: (f) => { self.frames.push(f); return true; }, binary: () => true });
        input!.take({ type: 'credit', n: self.credit });
        yield { type: 'data', metadata: { provenance: [], plexus_hash: '', timestamp: 0 }, content_type: 'svc.m', content: { n: 1 } } as PlexusStreamItem;
        await new Promise(() => {}); // the call stays open
      } finally {
        self.ended += 1;
      }
    })();
  }
}

function source(): { it: AsyncIterable<number>; pulled: () => number; returned: () => boolean } {
  let pulled = 0;
  let returned = false;
  const it: AsyncIterable<number> = {
    [Symbol.asyncIterator]() {
      return {
        async next() { await tick(1); pulled += 1; return { value: pulled, done: false }; },
        async return() { returned = true; return { value: undefined, done: true }; },
      };
    },
  };
  return { it, pulled: () => pulled, returned: () => returned };
}

test('pull: a call refused at open returns the source and pulls no more', async () => {
  const rpc = new FakeRpc(4, 'refuse');
  const s = source();
  const stream = pullInput<number>(rpc as any, 'svc.m', {}, false, s.it);
  await assert.rejects(stream.next(), /refused at open/);
  await tick(20);
  assert.ok(s.returned(), 'the source is returned');
  const at = s.pulled();
  await tick(20);
  assert.equal(s.pulled(), at, 'and pulled no more');
});

test('pull: a reader that lets go stops the input — no frame after it, the source returned', async () => {
  const rpc = new FakeRpc(1_000_000, 'data-then-wait');
  const s = source();
  for await (const item of pullInput<number>(rpc as any, 'svc.m', {}, false, s.it)) {
    assert.equal(item.type, 'data');
    await tick(20); // a few items go
    break;
  }
  const frames = rpc.frames.length;
  assert.ok(frames > 0);
  await tick(30);
  assert.equal(rpc.frames.length, frames, 'no frame to a call let go');
  assert.ok(s.returned(), 'the source is returned');
  assert.equal(rpc.ended, 1);
});

test('push: a write after the call ended is kept, never sent, and the end was said', async () => {
  const rpc = new FakeRpc(4, 'refuse');
  const call = pushInput<number, Promise<unknown>>(rpc as any, 'svc.m', {}, false, undefined, async (st) => { for await (const _ of st) { /* drain */ } });
  await assert.rejects(call.output, /refused at open/);
  call.input.write(1);
  call.input.write(2);
  assert.deepEqual(call.input.unsent(), [1, 2]);
  assert.equal(call.input.sent, 0);
  assert.equal(rpc.frames.length, 0);
  assert.ok(call.input.ended, 'the half says its call ended');
});

test('items are sent exactly as written: no key is renamed', async () => {
  const rpc = new FakeRpc(10, 'data-then-wait');
  const call = pushInput<unknown, AsyncGenerator<PlexusStreamItem>>(rpc as any, 'svc.m', {}, false, undefined, (st) => st);
  const item = { fooBar: 1, map: { 'Some Key': 'x', camelCase: 'y' }, list: [{ aB: 2 }] };
  call.input.write(item);
  await tick(10);
  assert.equal(rpc.frames.length, 1);
  assert.deepEqual(JSON.parse(rpc.frames[0]).params.items, [item]);
  await call.output.return(undefined);
});

test('an abort before the call starts opens no socket and says no disconnect', async () => {
  const rpc = new PlexusRpcClient({ backend: 'perf5', url: 'ws://127.0.0.1:1' });
  const ingest = Ingest.createIngestClient(rpc);
  const ac = new AbortController();
  ac.abort(new Error('not now'));
  const call = ingest.openItems({ signal: ac.signal });
  let said = 0;
  call.onDisconnect(() => { said += 1; });
  await assert.rejects(call.output, /not now/);
  await assert.rejects(ingest.items([{ at: 0, pad: 'x' }], { signal: ac.signal }), /not now/);
  await tick(20);
  assert.equal((rpc as any).ws, null, 'no socket');
  assert.equal(said, 0);
  const fake = new FakeRpc(4, 'data-then-wait');
  await assert.rejects(pullInput<number>(fake as any, 'svc.m', {}, false, [1], { signal: ac.signal }).next(), /not now/);
  assert.equal(fake.started, 0, 'the call never started');
});
