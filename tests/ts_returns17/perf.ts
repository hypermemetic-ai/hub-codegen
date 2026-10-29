// RETURNS-17 · perf: MethodStream's per-item cost over the bare generator it
// replaced (extractData), on 100,000 data items with no socket; median of 9.
// Run by tests/returns17_method_stream_test.rs (`perf_method_stream_overhead`,
// ignored by default); prints one JSON line.

import { MethodStream, extractData, type PlexusStreamItem } from '../index';

const N = 100_000;
const META = { provenance: ['hub'], plexus_hash: 'x', timestamp: 1 };

async function* wire(): AsyncGenerator<PlexusStreamItem> {
  for (let i = 0; i < N; i++) yield { type: 'data', metadata: META, content_type: 'alpha.a', content: { n: i } } as unknown as PlexusStreamItem;
  yield { type: 'done', metadata: META } as unknown as PlexusStreamItem;
}

async function bare(): Promise<number> {
  let sum = 0;
  for await (const x of extractData<{ n: number }>(wire())) sum += x.n;
  return sum;
}

async function stream(): Promise<number> {
  let sum = 0;
  for await (const x of new MethodStream<'alpha.a', { n: number }>('alpha.a', wire())) sum += x.n;
  return sum;
}

async function time(f: () => Promise<number>): Promise<number> {
  const t0 = process.hrtime.bigint();
  await f();
  return Number(process.hrtime.bigint() - t0) / N; // ns per item
}

const median = (xs: number[]) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

async function main(): Promise<void> {
  for (let i = 0; i < 3; i++) { await bare(); await stream(); }
  const b: number[] = [];
  const m: number[] = [];
  for (let i = 0; i < 9; i++) { b.push(await time(bare)); m.push(await time(stream)); }
  const mb = median(b);
  const mm = median(m);
  console.log(JSON.stringify({ items: N, bare_ns_per_item: +mb.toFixed(1), method_stream_ns_per_item: +mm.toFixed(1), overhead_ns_per_item: +(mm - mb).toFixed(1) }));
}

void main();
