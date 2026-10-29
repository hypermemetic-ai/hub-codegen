// RETURNS-38 · perf: the merge's cost per item over a bare stream (100,000
// items, three sources, no socket), and the heap a held source costs.
// Run by tests/returns38_watch_test.rs (`node --expose-gc out/.../perf.js`);
// prints one JSON line.

import { watch } from '../index';

const N = 100_000;

async function* count(from: number, to: number): AsyncGenerator<number> {
  for (let i = from; i < to; i++) yield i;
}

async function bare(): Promise<number> {
  let sum = 0;
  for await (const x of count(0, N)) sum += x;
  return sum;
}

async function merged(): Promise<number> {
  const third = Math.floor(N / 3);
  let sum = 0;
  for await (const it of watch(['a', count(0, third)], ['b', count(third, 2 * third)], ['c', count(2 * third, N)])) {
    if ('event' in it) sum += it.event;
  }
  return sum;
}

async function time(f: () => Promise<number>): Promise<number> {
  const t0 = process.hrtime.bigint();
  await f();
  return Number(process.hrtime.bigint() - t0) / 1000 / N; // µs per item
}

const median = (xs: number[]) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

/** A source that never speaks: parked forever, as a quiet watch is. */
function quiet(): AsyncIterable<never> {
  return { [Symbol.asyncIterator]: () => ({ next: () => new Promise<never>(() => {}), return: async () => ({ value: undefined, done: true as const }) }) };
}

async function main(): Promise<void> {
  for (let i = 0; i < 3; i++) { await bare(); await merged(); } // warm
  const b: number[] = [];
  const m: number[] = [];
  for (let i = 0; i < 9; i++) { b.push(await time(bare)); m.push(await time(merged)); }

  const gc = (globalThis as any).gc as (() => void) | undefined;
  const MERGES = 1000;
  const SOURCES = 3;
  // Baseline: the sources alone, held.
  gc?.(); const h0 = process.memoryUsage().heapUsed;
  const sources = Array.from({ length: MERGES }, () => [quiet(), quiet(), quiet()] as const);
  gc?.(); const h1 = process.memoryUsage().heapUsed;
  const merges = sources.map(([x, y, z]) => watch(['x', x], ['y', y], ['z', z]));
  const pending = merges.map((s) => s.next()); // every source subscribed and parked
  gc?.(); const h2 = process.memoryUsage().heapUsed;
  void pending;
  for (const s of merges) await s.close();

  console.log(JSON.stringify({
    items: N,
    bare_us_per_item: +median(b).toFixed(4),
    merged_us_per_item: +median(m).toFixed(4),
    overhead_us_per_item: +(median(m) - median(b)).toFixed(4),
    held_sources: MERGES * SOURCES,
    source_alone_bytes: Math.round((h1 - h0) / (MERGES * SOURCES)),
    merge_bytes_per_held_source: Math.round((h2 - h1) / (MERGES * SOURCES)),
    gc: gc !== undefined,
  }));
}

void main();
