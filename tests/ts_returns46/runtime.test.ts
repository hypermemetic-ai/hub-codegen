// RETURNS-46 · at run time (node --test): a call through a child accessor
// yields the nested aggregate from `.events()`, and `watch(...)` over such
// calls yields it too — the values the fixture's JSON fixes, the types
// `types.test.ts` checks. Every call's `.events()` is its client's
// aggregate: the root's own `{ method: 'count', event }`, exactly as Rust's
// `TreeEvent` serialises it, so a watch over a root call and a child call
// carries one shape; the path stays the watch item's `source`.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { watch, type RpcClient } from '../index';
import { createTreeClient } from '../tree/client';
import { createTreeBranchClient } from '../tree/branch/client';
import { createTreeBranchLeafClient } from '../tree/branch/leaf/client';
import type { TreeEvent } from '../tree/events';
import type { TreeBranchEvent } from '../tree/branch/events';
import { tree as treeLines, branch as branchLines, leaf as leafLines } from './lines';

const META = { provenance: ['tree_service'], plexus_hash: 'x', timestamp: 0 };

// What the service answers each path with: the fixture's calls.json, as items.
const ANSWERS: Record<string, unknown[]> = {
  'tree.count': [{ n: 1 }, { n: 2 }],
  'tree.branch.grow': [{ ok: { bud: 1 } }, { err: { type: 'frost', at: 2 } }, { ok: { bud: 3 } }],
  'tree.branch.leaf.fall': [{ leaf: 1 }, { leaf: 2 }],
};

const called: string[] = [];
const rpc: RpcClient = {
  async *call(method: string) {
    called.push(method);
    for (const content of ANSWERS[method] ?? []) {
      yield { type: 'data', metadata: META, content_type: method, content } as never;
    }
    yield { type: 'done', metadata: META } as never;
  },
};

async function all<T>(g: AsyncIterable<T>): Promise<T[]> {
  const out: T[] = [];
  for await (const x of g) out.push(x);
  return out;
}

const json = (xs: readonly unknown[]) => xs.map((x) => JSON.stringify(x));

test('a call through a child accessor yields the root aggregate', async () => {
  const tree = createTreeClient(rpc);
  const got: TreeEvent[] = [
    ...(await all(tree.branch.grow(3).events())),
    ...(await all(tree.branch.leaf.fall(2).events())),
  ];
  assert.deepEqual(json(got), json(treeLines.slice(3, 8)));
  assert.deepEqual(called.slice(-2), ['tree.branch.grow', 'tree.branch.leaf.fall']);
});

test("a level's own client yields its own aggregate", async () => {
  const branch = createTreeBranchClient(rpc, (e: TreeBranchEvent) => e);
  const got = [...(await all(branch.grow(3).events())), ...(await all(branch.leaf.fall(2).events()))];
  assert.deepEqual(json(got), json(branchLines));
  // made directly, with no lift: its own aggregate, and its children lift into it
  const direct = createTreeBranchClient(rpc);
  assert.deepEqual(json(await all(direct.leaf.fall(2).events())), json(branchLines.slice(3)));
  assert.deepEqual(json(await all(direct.grow(3).events())), json(branchLines.slice(0, 3)));
});

test("every call's events are its client's aggregate: the root's, a leaf made directly", async () => {
  const leaf = createTreeBranchLeafClient(rpc);
  assert.deepEqual(json(await all(leaf.fall(2).events())), json(leafLines));
  const tree = createTreeClient(rpc);
  assert.deepEqual(json(await all(tree.count(2).events())), json(treeLines.slice(1, 3)));
});

test('a watch over a root call and a child call carries one shape', async () => {
  const tree = createTreeClient(rpc);
  const items = await all(watch(tree.count(2), tree.branch.grow(3), tree.branch.leaf.fall(2)));
  const bySource = new Map<string, string[]>();
  for (const it of items) {
    assert.ok('event' in it, JSON.stringify(it));
    bySource.set(it.source, [...(bySource.get(it.source) ?? []), JSON.stringify(it.event)]);
  }
  // every event a line of aggregate.tree.jsonl; the path is the item's source
  assert.deepEqual(bySource.get('tree.count'), json(treeLines.slice(1, 3)));
  assert.deepEqual(bySource.get('tree.branch.grow'), json(treeLines.slice(3, 6)));
  assert.deepEqual(bySource.get('tree.branch.leaf.fall'), json(treeLines.slice(6, 8)));
});

test('watch over lifted calls yields the nested aggregate', async () => {
  const tree = createTreeClient(rpc);
  const items = await all(watch(tree.branch.grow(3), tree.branch.leaf.fall(2)));
  const events = items.map((it) => {
    assert.ok('event' in it, JSON.stringify(it));
    return it.event;
  });
  assert.deepEqual(json(events).sort(), json(treeLines.slice(3, 8)).sort());
  const sources = new Set(items.map((it) => it.source));
  assert.deepEqual([...sources].sort(), ['tree.branch.grow', 'tree.branch.leaf.fall']);
});
