// RETURNS-46 · the TypeScript aggregate nests as the Rust one does, checked
// by `tsc` (never run). Copied beside a package generated from
// tests/fixtures/returns46_tree_ir.json — plexus-macros'
// fixtures/aggregate-tree, as its service writes the IR: `tree` (ping,
// count), its static child `tree.branch` (grow, a Result item), that one's
// static child `tree.branch.leaf` (fall), and the dynamic child `tree.item`.
// `lines.ts` beside this file holds the fixture's aggregate JSON, each line
// typed as its level's aggregate. Every `@ts-expect-error` below is an
// assertion: `tsc` fails if the line it covers is accepted.

import { expectTypeOf } from 'expect-type';
import { Tree, TreeBranchLeaf, watch, type Result, type RpcClient, type MethodEvent, type Tagged, type Merged } from '../index';
import type { TreeEvent } from '../tree/events';
import type { TreeBranchEvent } from '../tree/branch/events';
import type { TreeBranchLeafEvent } from '../tree/branch/leaf/events';
import type { Pong, Tick } from '../tree/types';
import type { Bud, GrowError } from '../tree/branch/types';
import type { Fell } from '../tree/branch/leaf/types';

// ── each level's aggregate is exported from its namespace ──────────────────
expectTypeOf<Tree.TreeEvent>().toEqualTypeOf<TreeEvent>();

// ── the variants, exactly ──────────────────────────────────────────────────
expectTypeOf<TreeBranchLeafEvent>().toEqualTypeOf<{ method: 'fall'; event: Fell }>();
expectTypeOf<TreeBranchEvent>().toEqualTypeOf<
  { method: 'grow'; event: Result<Bud, GrowError> } | { method: 'leaf'; event: TreeBranchLeafEvent }
>();
expectTypeOf<Extract<TreeEvent, { method: 'ping' }>>().toEqualTypeOf<{ method: 'ping'; event: Pong }>();
expectTypeOf<Extract<TreeEvent, { method: 'count' }>>().toEqualTypeOf<{ method: 'count'; event: Tick }>();
expectTypeOf<Extract<TreeEvent, { method: 'branch' }>>().toEqualTypeOf<{ method: 'branch'; event: TreeBranchEvent }>();
// the dynamic child: the instance's name, and an event the IR cannot type yet
expectTypeOf<Extract<TreeEvent, { method: 'item' }>>().toEqualTypeOf<
  { method: 'item'; event: { name: string; event: unknown } }
>();
// the auto `schema` method is not an item of the aggregate, as in Rust
expectTypeOf<Extract<TreeEvent, { method: 'schema' }>>().toBeNever();

// ── a wrong nesting is refused ─────────────────────────────────────────────
// @ts-expect-error — a grandchild's event straight under the root
export const w1: TreeEvent = { method: 'fall', event: { leaf: 1 } };
// @ts-expect-error — the leaf one level too high
export const w2: TreeEvent = { method: 'leaf', event: { method: 'fall', event: { leaf: 1 } } };
// @ts-expect-error — a dynamic instance's event without its name
export const w3: TreeEvent = { method: 'item', event: { event: { method: 'watch', event: {} } } };
// @ts-expect-error — a method of the branch under the leaf
export const w4: TreeEvent = { method: 'branch', event: { method: 'leaf', event: { method: 'grow', event: { ok: { bud: 1 } } } } };
// @ts-expect-error — a Result item is { ok } | { err }, never the bare item
export const w5: TreeBranchEvent = { method: 'grow', event: { bud: 1 } };
// @ts-expect-error — the branch's event is its aggregate, not the item
export const w6: TreeEvent = { method: 'branch', event: { bud: 1 } };

// ── one listener, three levels, matched exhaustively ───────────────────────
export function reduce(ev: TreeEvent): string {
  switch (ev.method) {
    case 'ping':
      return ev.event.message;
    case 'count':
      return `tick ${ev.event.n}`;
    case 'branch':
      switch (ev.event.method) {
        case 'grow':
          return 'ok' in ev.event.event ? `bud ${ev.event.event.ok.bud}` : `frost ${ev.event.event.err.at}`;
        case 'leaf':
          return `leaf ${ev.event.event.event.leaf}`;
        default: {
          const never: never = ev.event;
          return never;
        }
      }
    case 'item':
      return `item ${ev.event.name}`;
    default: {
      const never: never = ev;
      return never;
    }
  }
}

// ── at run time: a call through an accessor is the root's aggregate ─────────
export function liftedTypes(rpc: RpcClient): void {
  const tree = Tree.createTreeClient(rpc);
  expectTypeOf(tree.branch.grow(1).events()).toEqualTypeOf<AsyncGenerator<TreeEvent, undefined>>();
  expectTypeOf(tree.branch.leaf.fall(1).events()).toEqualTypeOf<AsyncGenerator<TreeEvent, undefined>>();
  // the root's own call: its client's aggregate too, not { method: '<path>' }
  expectTypeOf(tree.count(1).events()).toEqualTypeOf<AsyncGenerator<TreeEvent, undefined>>();
  // @ts-expect-error — the path form is gone from .events()
  expectTypeOf(tree.count(1).events()).toEqualTypeOf<AsyncGenerator<MethodEvent<'tree.count', Tick>, undefined>>();
  // a client made directly: its own aggregate
  expectTypeOf(TreeBranchLeaf.createTreeBranchLeafClient(rpc).fall(1).events()).toEqualTypeOf<AsyncGenerator<TreeBranchLeafEvent, undefined>>();
  // @ts-expect-error — through an accessor it is not the child's own event any more
  expectTypeOf(tree.branch.grow(1).events()).toEqualTypeOf<AsyncGenerator<MethodEvent<'tree.branch.grow', Bud>, undefined>>();
  // watch over lifted calls: each item the nested aggregate, tagged by its source
  const merged = watch(tree.branch.grow(1), tree.branch.leaf.fall(1));
  expectTypeOf(merged).toEqualTypeOf<
    Merged<Tagged<'tree.branch.grow', TreeEvent> | Tagged<'tree.branch.leaf.fall', TreeEvent>>
  >();
  // a root call and a child call: one shape
  const mixed = watch(tree.count(1), tree.branch.grow(1));
  expectTypeOf(mixed).toEqualTypeOf<Merged<Tagged<'tree.count', TreeEvent> | Tagged<'tree.branch.grow', TreeEvent>>>();
}
