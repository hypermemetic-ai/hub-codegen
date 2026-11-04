// RETURNS-46 · at run time (node --test): a call through a child accessor
// yields the nested aggregate from `.events()`, and `watch(...)` over such
// calls yields it too — the values the fixture's JSON fixes, the types
// `types.test.ts` checks. A client made directly, and the root's own calls,
// are as RETURNS-17 left them: `{ method: '<path>', event }`.

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
  // made directly, with no lift: as RETURNS-17 left it, but its children lift into it
  const direct = createTreeBranchClient(rpc);
  assert.deepEqual(json(await all(direct.leaf.fall(2).events())), json(branchLines.slice(3)));
  assert.deepEqual(json(await all(direct.grow(1).events())).slice(0, 1), [JSON.stringify({ method: 'tree.branch.grow', event: { ok: { bud: 1 } } })]);
});

test('a client made directly, and the root, are unchanged', async () => {
  const leaf = createTreeBranchLeafClient(rpc);
  assert.deepEqual(json(await all(leaf.fall(2).events())), json(leafLines.map((l) => ({ method: 'tree.branch.leaf.fall', event: l.event }))));
  const tree = createTreeClient(rpc);
  assert.deepEqual(json(await all(tree.count(2).events())), json([{ method: 'tree.count', event: { n: 1 } }, { method: 'tree.count', event: { n: 2 } }]));
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
