// BIDIR-19 · the types of a method with an input half, checked by `tsc
// --strict` (tests/bidir19_input_test.rs). Each `@ts-expect-error` is an
// assertion: an unused one fails the compile.

import { expectTypeOf } from 'expect-type';
import {
  Ingest,
  MethodStream,
  PlexusRpcClient,
  DisconnectedError,
  PastCreditError,
  InvalidInputError,
  InputClosedError,
  RemoteCallError,
  type DuplexCall,
  type DisconnectInfo,
  type InputWriter,
} from '../index';

declare const rpc: PlexusRpcClient;
const c = Ingest.createIngestClient(rpc);

// pull: the input is the last argument, any (async) iterable of the item
expectTypeOf(c.items([{ at: 1, pad: 'x' }])).toEqualTypeOf<Promise<Ingest.IngestEvent>>();
async function* chunks(): AsyncGenerator<Uint8Array> { yield new Uint8Array(4); }
expectTypeOf(c.bytes(chunks())).toEqualTypeOf<Promise<Ingest.IngestEvent>>();
expectTypeOf(c.tokens('r1', undefined, { team: 'a' }, ['a', 'b'], { pressureAt: 10 })).toEqualTypeOf<MethodStream<'ingest.tokens', Ingest.IngestEvent, Ingest.IngestAggregate>>(); // RETURNS-46: its events the client's aggregate
// @ts-expect-error the input is required
c.items();
// @ts-expect-error an Input<Bytes> takes Uint8Array, not strings
c.bytes(['abc']);
// @ts-expect-error an Item has `at` and `pad`
c.items([{ at: 1 }]);
// @ts-expect-error an optional param before the input is written, if only as undefined
c.tokens('r1', undefined, ['a']);

// push: open<Method>() → { input, output, onDisconnect }
const b = c.openBytes({ pressureAt: 100, signal: new AbortController().signal });
expectTypeOf(b).toEqualTypeOf<DuplexCall<Uint8Array, Promise<Ingest.IngestEvent>>>();
expectTypeOf(b.input).toEqualTypeOf<InputWriter<Uint8Array>>();
expectTypeOf(b.input.write).returns.toEqualTypeOf<void>();
expectTypeOf(b.input.buffered).toEqualTypeOf<number>();
expectTypeOf(b.input.unsent()).toEqualTypeOf<Uint8Array[]>();
b.input.write(new Uint8Array(640));
// @ts-expect-error bytes, not a string
b.input.write('x');
b.onDisconnect((d) => { expectTypeOf(d).toEqualTypeOf<DisconnectInfo>(); expectTypeOf(d.unsent).toEqualTypeOf<number>(); });
expectTypeOf(c.openItems().input.write).parameter(0).toEqualTypeOf<Ingest.Item>();
expectTypeOf(c.openTokens('r1').output).toEqualTypeOf<MethodStream<'ingest.tokens', Ingest.IngestEvent, Ingest.IngestAggregate>>();
// @ts-expect-error a push call has no input argument
c.openItems([{ at: 1, pad: 'x' }]);
// @ts-expect-error `run` is required
c.openTokens();

// the errors: a disconnect carries the counts; the input's refusals are RemoteCallErrors
declare const e: unknown;
if (e instanceof DisconnectedError) expectTypeOf(e.sent).toEqualTypeOf<number>();
if (e instanceof PastCreditError) { expectTypeOf(e.credit).toEqualTypeOf<number>(); expectTypeOf(e).toMatchTypeOf<RemoteCallError>(); }
if (e instanceof InvalidInputError) expectTypeOf(e.seq).toEqualTypeOf<number>();
if (e instanceof InputClosedError) expectTypeOf(e.reason).toEqualTypeOf<string>();
