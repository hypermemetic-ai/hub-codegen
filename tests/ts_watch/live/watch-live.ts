// RETURNS-38 · watch(a, b, c) against a throwaway menger, through the
// TypeScript client service-codegen generates from its IR, every frame counted
// by frame-proxy.mjs (spawned here). Not part of `cargo test`: it needs a
// running menger and a key it admits. To run it:
//
//   synapse -P 4484 -i menger > ir.json
//   service-codegen ir.json -o pkg --transport browser
//   cp watch-live.ts frame-proxy.mjs pkg/watch-live/ && compile with tsc (CommonJS)
//   MENGER_DEPLOY_KEY_FILE=~/.plexus/keys/builder.key FRAME_PROXY=pkg/watch-live/frame-proxy.mjs \
//     MENGER_CALL=…/menger/deploy/menger-call.mjs node out/watch-live/watch-live.js
//
// Prints one JSON line: what arrived, in order, and the frames each half sent.

import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { watch, PlexusRpcClient, Tickets } from '../index';

const UPSTREAM = Number(process.env.MENGER_PORT ?? 4484);
const LISTEN = Number(process.env.PROXY_PORT ?? 4486);
const frames: any[] = [];

async function proxy(): Promise<() => void> {
  const child = spawn(process.execPath, [process.env.FRAME_PROXY!, '--listen', String(LISTEN), '--upstream', String(UPSTREAM), '--agent', 'builder', '--call-module', process.env.MENGER_CALL!], { stdio: ['ignore', 'pipe', 'inherit'] });
  createInterface({ input: child.stdout! }).on('line', (l) => frames.push(JSON.parse(l).frame));
  await new Promise((r) => setTimeout(r, 500));
  return () => child.kill();
}

const tick = (ms: number) => new Promise((r) => setTimeout(r, ms));
const methodOf = (f: any) => f.method === 'menger.call' ? `call ${f.params.method}` : f.method;
function count(from: number): Record<string, number> {
  const c: Record<string, number> = {};
  for (const f of frames.slice(from)) c[methodOf(f)] = (c[methodOf(f)] ?? 0) + 1;
  return c;
}

async function main(): Promise<void> {
  const stop = await proxy();
  const rpc = new PlexusRpcClient({ backend: 'menger', debug: !!process.env.DEBUG, url: `ws://127.0.0.1:${LISTEN}` });
  await rpc.connect();
  const t = Tickets.createTicketsClient(rpc);
  const out: Record<string, unknown> = {};

  // 1. order, end, error: an open watch, a finite list, a get that fails.
  let from = frames.length;
  const s = watch(['tickets.watch', t.watch('LIVE')], ['tickets.list', t.list(undefined, undefined, 'LIVE')], ['tickets.get', t.get('not an id!')]);
  const arrived: string[] = [];
  let created = false;
  for await (const it of s) {
    if ('error' in it) arrived.push(`${it.source}: failed (${it.error.constructor.name}: ${it.error.message.slice(0, 40)}…)`);
    else arrived.push(`${it.source}: ${(it.event as any).type}`);
    if (!created && arrived.filter((a) => a.startsWith('tickets.list') || a.startsWith('tickets.get')).length >= 2) {
      created = true;
      // a write on its own call, so the watch has something to say
      for await (const _ of rpc.call('tickets.create', { epic: 'LIVE', title: `watch live ${Date.now()}`, parent: 'LIVE-1' })) { /* drain */ }
    }
    if (arrived.some((a) => a === 'tickets.watch: created')) break; // a break: the watch is the one still open
  }
  await tick(300);
  out.arrived = arrived;
  out.frames_order_end_error = count(from);

  // 2. three open-ended streams, merged then closed, vs by hand. A write
  // under LIVE-1 makes both watches speak; await_reply says `waiting` at once.
  const create = async () => { for await (const _ of rpc.call('tickets.create', { epic: 'LIVE', title: `watch live ${Date.now()}`, parent: 'LIVE-1' })) { /* drain */ } };
  const three = () => [t.watch('LIVE'), t.awaitReply('LIVE-1'), t.watch(undefined, 'under LIVE-1')] as const;

  from = frames.length;
  const [w1, r1, w2] = three();
  const m = watch(['tickets.watch', w1], ['tickets.await_reply', r1], ['tickets.watch', w2]);
  const firsts: string[] = [];
  const first = m.next();
  await tick(300); // all three subscribed
  await create();
  firsts.push(JSON.stringify((await first).value).slice(0, 60));
  for (let i = 0; i < 2; i++) firsts.push(JSON.stringify((await m.next()).value).slice(0, 60));
  await m.close();
  await tick(300);
  out.merged_first_items = firsts;
  out.frames_merged = count(from).valueOf();

  from = frames.length;
  const gens = three();
  const pending = gens.map((g) => g.next());
  await tick(300);
  await create();
  await Promise.all(pending);
  for (const g of gens) await g.return(undefined);
  await tick(300);
  out.frames_by_hand = count(from);

  console.log(JSON.stringify(out));
  rpc.disconnect();
  stop();
}

main().catch((e) => { console.error(e); process.exit(1); });
