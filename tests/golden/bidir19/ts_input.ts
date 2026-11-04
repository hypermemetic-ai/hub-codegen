// The input half of a call (BIDIR-19; plexus-wire spec/10)
//
// A method with an input half takes a stream in beside its params: items sent
// as `{ns}.input {subscription, seq, items}` (an Input<Bytes> item as one
// binary message `[0x01][subscription u64 BE][seq u64 BE][payload]`), under
// the credit the service grants, and `{ns}.end {subscription, seq}` when the
// stream ends. The service's `credit`, `ack` and `input_closed` items are
// taken here and never reach the output.
//
// The client's rules: the service never refuses a client that obeys credit;
// an item is buffered here until credit allows, never dropped; a disconnect
// is always said — `onDisconnect({ sent, acked, unsent })` and a
// DisconnectedError on the output — and nothing is resent (no resume: a call
// dies with its socket). What was not sent stays readable (`unsent()`), and
// what was sent but not acknowledged too (`unacked()`): the application decides.

import type { PlexusStreamItem, StreamMetadata } from './types';
import type { RpcClient, RemoteError } from './rpc';
import { RemoteCallError } from './rpc';

/** Options of a call with an input half. */
export interface InputOptions {
  /**
   * Items buffered here (written, not yet sent) at which `onPressure` fires:
   * the service is behind. Default 1,000. Nothing is refused past it.
   */
  readonly pressureAt?: number;
  /** Aborts the call: it is unsubscribed, and its output throws the signal's reason. */
  readonly signal?: AbortSignal;
}

/** The service closed the input half (`input_closed`): it reads no more. */
export interface InputClosed {
  /** `method_ended` (the method ended first), or the `error_kind` of the refusal that follows. */
  readonly reason: string;
  /** The last frame the service consumed, when it consumed any. */
  readonly ack?: number;
}

/** What the input half had done when the socket under it dropped. Counts are items. */
export interface DisconnectInfo {
  /** The method's path. */
  readonly method: string;
  /** Items sent on the socket. */
  readonly sent: number;
  /** Items the service has acknowledged consuming (every frame through its last `ack`). */
  readonly acked: number;
  /** Items written and never sent: `input.unsent()`. */
  readonly unsent: number;
  /** Items sent and never acknowledged: `input.unacked()`. The service may or may not have consumed them. */
  readonly unacked: number;
  /** Input frames sent. */
  readonly frames: number;
  /** The last frame acknowledged, or null when none was. */
  readonly ackSeq: number | null;
}

/** `onPressure`'s report. */
export interface Pressure {
  readonly buffered: number;
  readonly pressureAt: number;
}

/** The input half of an open call: written to, never refusing. */
export interface InputWriter<T> {
  /** Queue an item: sent when credit allows. Never refused, never dropped; the queue has no cap. */
  write(item: T): void;
  /** The input ends: `{ns}.end` once every queued item is sent. Writing after it is a mistake, and throws. */
  end(): void;
  /** Items written and not yet sent. */
  readonly buffered: number;
  /** Items the service will take now. */
  readonly credit: number;
  /** Items sent. */
  readonly sent: number;
  /** Items the service has acknowledged consuming. */
  readonly acked: number;
  /** Set once the service closed the half. */
  readonly closed: InputClosed | undefined;
  /**
   * The call is over — it answered, failed, was refused, dropped, aborted or
   * let go by its reader: nothing more is sent. A write after it is kept
   * (`unsent()`); how it ended is what the output (and, for a dropped
   * socket, `onDisconnect`) said.
   */
  readonly ended: boolean;
  /** Fires once each time `buffered` reaches `pressureAt` from below. Returns its removal. */
  onPressure(handler: (pressure: Pressure) => void): () => void;
  /** The items written and never sent, in order. */
  unsent(): T[];
  /** The items sent and not acknowledged, in order. */
  unacked(): T[];
}

/** An open call with an input half: what `open<Method>()` answers. */
export interface DuplexCall<T, O> {
  /** Write the input here. */
  readonly input: InputWriter<T>;
  /** The method's answer: a Promise of its one item, or a MethodStream of many. */
  readonly output: O;
  /** Fires when the socket under the call drops — every time, also when it already has. Returns its removal. */
  onDisconnect(handler: (info: DisconnectInfo) => void): () => void;
}

/**
 * The socket dropped under a call with an input half: what its output throws.
 * Carries what `onDisconnect` was told. Nothing was resent.
 */
export class DisconnectedError extends Error implements DisconnectInfo {
  readonly method: string;
  readonly sent: number;
  readonly acked: number;
  readonly unsent: number;
  readonly unacked: number;
  readonly frames: number;
  readonly ackSeq: number | null;
  constructor(info: DisconnectInfo) {
    super(
      `${info.method}: the connection dropped — ${info.sent} items sent, ${info.acked} acknowledged, ` +
      `${info.unsent} never sent (input.unsent()); nothing was resent`
    );
    this.name = 'DisconnectedError';
    this.method = info.method;
    this.sent = info.sent;
    this.acked = info.acked;
    this.unsent = info.unsent;
    this.unacked = info.unacked;
    this.frames = info.frames;
    this.ackSeq = info.ackSeq;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

type ErrorItem = { message: string; code?: string; recoverable: boolean; metadata?: StreamMetadata; [field: string]: unknown };

/** The service refused the call's input (spec/10): a RemoteCallError, typed by its `error_kind`. */
export class InputError extends RemoteCallError {
  constructor(method: string, item: ErrorItem) {
    const { type: _type, metadata, ...rest } = item as ErrorItem & { type?: unknown };
    super(method, { ...rest, code: item.code ?? '' } as unknown as RemoteError, metadata);
    this.name = 'InputError';
    Object.setPrototypeOf(this, new.target.prototype);
  }

  // RemoteCallError answers `instanceof` for every remote error; a subclass
  // answers for its own instances only.
  static [Symbol.hasInstance]<C>(this: abstract new (...args: never[]) => C, x: unknown): x is C {
    return Function.prototype[Symbol.hasInstance].call(this, x);
  }
}

const num = (v: unknown): number => (typeof v === 'number' ? v : Number(v));

/** `past_credit {seq, credit}`: a frame went past the credit granted. This client's bug, never the application's. */
export class PastCreditError extends InputError {
  readonly seq: number;
  readonly credit: number;
  constructor(method: string, item: ErrorItem) {
    super(method, item);
    this.name = 'PastCreditError';
    this.seq = num(item.seq);
    this.credit = num(item.credit);
  }
}

/** `out_of_sequence {seq, expected}`: a frame's `seq` was not the next. */
export class OutOfSequenceError extends InputError {
  readonly seq: number;
  readonly expected: number;
  constructor(method: string, item: ErrorItem) {
    super(method, item);
    this.name = 'OutOfSequenceError';
    this.seq = num(item.seq);
    this.expected = num(item.expected);
  }
}

/** `invalid_input {seq}`: an item of frame `seq` is not the method's input type. */
export class InvalidInputError extends InputError {
  readonly seq: number;
  constructor(method: string, item: ErrorItem) {
    super(method, item);
    this.name = 'InvalidInputError';
    this.seq = num(item.seq);
  }
}

/** `too_much_input {limit, max}`: the method takes at most `max` `limit` (items or bytes). */
export class TooMuchInputError extends InputError {
  readonly limit: string;
  readonly max: number;
  constructor(method: string, item: ErrorItem) {
    super(method, item);
    this.name = 'TooMuchInputError';
    this.limit = String(item.limit);
    this.max = num(item.max);
  }
}

/**
 * `input_closed` and a refusal: the service stopped reading the input and
 * ended the call — `reason` is the refusal's kind (`forbidden`,
 * `unauthenticated`, …), and `error` the refusal itself.
 */
export class InputClosedError extends InputError {
  readonly reason: string;
  readonly ack: number | undefined;
  constructor(method: string, item: ErrorItem, closed: InputClosed | undefined) {
    super(method, item);
    this.name = 'InputClosedError';
    this.reason = closed?.reason ?? String(item.error_kind ?? 'input_closed');
    this.ack = closed?.ack;
  }
}

/** Where an input half sends: the call's socket, as the transport hands it over. */
export interface InputWire {
  readonly backend: string;
  readonly subscription: number;
  /** Send a text frame; false when the socket is no longer open. */
  text(frame: string): boolean;
  /** Send a binary frame; false when the socket is no longer open. */
  binary(frame: Uint8Array): boolean;
}

/** The most items one `{ns}.input` carries, and about the most bytes. */
const FRAME_ITEMS = 256;
const FRAME_BYTES = 1024 * 1024;

/** `[0x01][subscription u64 BE][seq u64 BE][payload]` (BIDIR-17). */
function binaryFrame(subscription: number, seq: number, payload: Uint8Array): Uint8Array {
  const frame = new Uint8Array(17 + payload.byteLength);
  const view = new DataView(frame.buffer);
  frame[0] = 0x01;
  view.setBigUint64(1, BigInt(subscription));
  view.setBigUint64(9, BigInt(seq));
  frame.set(payload, 17);
  return frame;
}

function report(e: unknown): void {
  const r = (globalThis as { reportError?: (e: unknown) => void }).reportError;
  if (r) r(e); else console.error(e);
}

/**
 * The input half of one call. The transport opens it on the call's socket,
 * hands it the call's items (`take`) and its socket's loss (`lost`); the
 * generated method writes to it, or drains an iterable into it.
 */
export class InputHalf<T> implements InputWriter<T> {
  readonly method: string;
  /** Items are bytes: one binary frame each. */
  readonly binary: boolean;
  readonly pressureAt: number;
  #queue: T[] = [];
  #head = 0;
  /** Frames sent and not acknowledged, oldest first. */
  #inflight: Array<{ seq: number; items: T[] }> = [];
  #credit = 0;
  #seq = 0;
  #sent = 0;
  #acked = 0;
  #ackSeq: number | null = null;
  #wire: InputWire | undefined;
  #ending = false;
  #endSent = false;
  #closed: InputClosed | undefined;
  /** The call's terminal item came. */
  #finished = false;
  /** The call is over, however it ended. */
  #callEnded = false;
  #disconnected: DisconnectInfo | undefined;
  #aborted = false;
  #abortReason: unknown;
  #pressured = false;
  #pressureHandlers = new Set<(p: Pressure) => void>();
  #disconnectHandlers = new Set<(d: DisconnectInfo) => void>();
  #cancel: (() => void) | undefined;
  #waiters: Array<() => void> = [];
  #unlisten: (() => void) | undefined;

  constructor(method: string, binary: boolean, options?: InputOptions) {
    this.method = method;
    this.binary = binary;
    this.pressureAt = options?.pressureAt ?? 1000;
    const signal = options?.signal;
    if (signal) {
      if (signal.aborted) this.abort(signal.reason);
      else {
        const onAbort = (): void => this.abort(signal.reason);
        signal.addEventListener('abort', onAbort, { once: true });
        this.#unlisten = () => signal.removeEventListener('abort', onAbort);
      }
    }
  }

  get buffered(): number { return this.#queue.length - this.#head; }
  get credit(): number { return this.#credit; }
  get sent(): number { return this.#sent; }
  get acked(): number { return this.#acked; }
  get closed(): InputClosed | undefined { return this.#closed; }
  get ended(): boolean { return this.#callEnded || this.#finished || this.#disconnected !== undefined || this.#aborted; }
  /** The socket dropped under the call: what was said. */
  get disconnected(): DisconnectInfo | undefined { return this.#disconnected; }
  get aborted(): boolean { return this.#aborted; }
  get abortReason(): unknown { return this.#abortReason; }

  write(item: T): void {
    if (this.#ending) throw new TypeError(`${this.method}: input written after end()`);
    this.#queue.push(item);
    if (!this.#pressured && this.buffered >= this.pressureAt) {
      this.#pressured = true;
      const p: Pressure = { buffered: this.buffered, pressureAt: this.pressureAt };
      for (const h of this.#pressureHandlers) { try { h(p); } catch (e) { report(e); } }
    }
    this.#pump();
  }

  end(): void {
    this.#ending = true;
    this.#pump();
  }

  onPressure(handler: (pressure: Pressure) => void): () => void {
    this.#pressureHandlers.add(handler);
    return () => { this.#pressureHandlers.delete(handler); };
  }

  onDisconnect(handler: (info: DisconnectInfo) => void): () => void {
    this.#disconnectHandlers.add(handler);
    const said = this.#disconnected;
    if (said) queueMicrotask(() => { if (this.#disconnectHandlers.has(handler)) { try { handler(said); } catch (e) { report(e); } } });
    return () => { this.#disconnectHandlers.delete(handler); };
  }

  unsent(): T[] { return this.#queue.slice(this.#head); }

  unacked(): T[] { return this.#inflight.flatMap((f) => f.items); }

  // ─── the transport's side ────────────────────────────────────────────────

  /** @internal The call is acknowledged: send on its socket. */
  open(wire: InputWire): void {
    this.#wire = wire;
    this.#pump();
  }

  /** @internal Cancel the call (unsubscribe): set by the transport, run on abort. */
  bindCancel(cancel: () => void): void {
    this.#cancel = cancel;
    if (this.#aborted) cancel();
  }

  /** @internal One of the call's items: true when it is the input half's (credit, ack, input_closed). */
  take(item: unknown): boolean {
    const it = item as { type?: unknown; n?: unknown; seq?: unknown; reason?: unknown; ack?: unknown };
    switch (it.type) {
      case 'credit':
        this.#credit += num(it.n);
        this.#pump();
        this.#wake();
        return true;
      case 'ack':
        this.#ackThrough(num(it.seq));
        return true;
      case 'input_closed': {
        const ack = it.ack === undefined || it.ack === null ? undefined : num(it.ack);
        this.#closed = ack === undefined ? { reason: String(it.reason) } : { reason: String(it.reason), ack };
        if (ack !== undefined) this.#ackThrough(ack);
        this.#wake();
        return true;
      }
      case 'done':
      case 'error':
        this.#finished = true;
        this.#unlisten?.();
        this.#wake();
        return false;
      default:
        return false;
    }
  }

  /** @internal The call is over, however it ended: send no more, pull no more, listen no more. */
  callEnded(): void {
    if (this.#callEnded) return;
    this.#callEnded = true;
    this.#unlisten?.();
    this.#wake();
  }

  /** @internal The socket under the call dropped (or the call was never acknowledged). Said once. */
  lost(): void {
    if (this.#finished || this.#disconnected || this.#callEnded) return;
    const info: DisconnectInfo = {
      method: this.method,
      sent: this.#sent,
      acked: this.#acked,
      unsent: this.buffered,
      unacked: this.#sent - this.#acked,
      frames: this.#seq,
      ackSeq: this.#ackSeq,
    };
    this.#disconnected = info;
    this.#unlisten?.();
    for (const h of this.#disconnectHandlers) { try { h(info); } catch (e) { report(e); } }
    this.#wake();
  }

  /** @internal The typed error an error item of this call is, when it is the input half's. */
  errorFor(item: ErrorItem): InputError | undefined {
    switch (item.error_kind) {
      case 'past_credit': return new PastCreditError(this.method, item);
      case 'out_of_sequence': return new OutOfSequenceError(this.method, item);
      case 'invalid_input': return new InvalidInputError(this.method, item);
      case 'too_much_input': return new TooMuchInputError(this.method, item);
      case 'input_closed': return new InputClosedError(this.method, item, this.#closed);
      default:
        if (this.#closed && this.#closed.reason !== 'method_ended') return new InputClosedError(this.method, item, this.#closed);
        return undefined;
    }
  }

  /** Abort the call: nothing more is sent; the output throws `reason`. */
  abort(reason?: unknown): void {
    if (this.#aborted || this.#finished || this.#callEnded) return;
    this.#aborted = true;
    this.#abortReason = reason ?? new Error(`${this.method}: aborted`);
    this.#cancel?.();
    this.#wake();
  }

  /** Drain `source` into the half, pulling only while credit is unspent; `end()` when it ends. */
  async pullFrom(source: AsyncIterable<T> | Iterable<T>): Promise<void> {
    const it: AsyncIterator<T> | Iterator<T> =
      Symbol.asyncIterator in source
        ? (source as AsyncIterable<T>)[Symbol.asyncIterator]()
        : (source as Iterable<T>)[Symbol.iterator]();
    let done = false;
    try {
      for (;;) {
        await this.#room();
        if (this.#over()) return;
        const r = await it.next();
        if (r.done) { done = true; this.end(); return; }
        this.write(r.value);
      }
    } catch (e) {
      done = true;
      this.abort(e);
    } finally {
      if (!done) await it.return?.();
    }
  }

  // ─── within ──────────────────────────────────────────────────────────────

  #over(): boolean {
    return this.#finished || this.#callEnded || this.#closed !== undefined || this.#disconnected !== undefined || this.#aborted;
  }

  /** Resolves once one more item would be within credit, or the half is over. */
  #room(): Promise<void> {
    if (this.#credit > this.buffered || this.#over()) return Promise.resolve();
    return new Promise<void>((resolve) => { this.#waiters.push(resolve); });
  }

  #wake(): void {
    const waiters = this.#waiters;
    this.#waiters = [];
    for (const w of waiters) w();
  }

  #ackThrough(seq: number): void {
    if (this.#ackSeq !== null && seq <= this.#ackSeq) return;
    this.#ackSeq = seq;
    while (this.#inflight.length > 0 && this.#inflight[0].seq <= seq) {
      this.#acked += this.#inflight.shift()!.items.length;
    }
  }

  /** Send what credit allows; `end` once the input has ended and nothing is queued. */
  #pump(): void {
    const wire = this.#wire;
    if (!wire || this.#over()) return;
    while (this.#credit > 0 && this.buffered > 0) {
      if (this.binary) {
        const item = this.#queue[this.#head] as unknown as Uint8Array;
        if (!wire.binary(binaryFrame(wire.subscription, this.#seq, item))) return;
        this.#head += 1;
        this.#credit -= 1;
        this.#inflight.push({ seq: this.#seq, items: [item as unknown as T] });
        this.#seq += 1;
        this.#sent += 1;
        continue;
      }
      const most = Math.min(this.#credit, this.buffered, FRAME_ITEMS);
      const parts: string[] = [];
      let bytes = 0;
      while (parts.length < most && bytes < FRAME_BYTES) {
        const s = JSON.stringify(this.#queue[this.#head + parts.length]) ?? 'null';
        parts.push(s);
        bytes += s.length;
      }
      const frame =
        `{"jsonrpc":"2.0","method":${JSON.stringify(`${wire.backend}.input`)},"params":` +
        `{"subscription":${JSON.stringify(wire.subscription)},"seq":${this.#seq},"items":[${parts.join(',')}]}}`;
      if (!wire.text(frame)) return;
      const items = this.#queue.slice(this.#head, this.#head + parts.length);
      this.#head += parts.length;
      this.#credit -= parts.length;
      this.#inflight.push({ seq: this.#seq, items });
      this.#seq += 1;
      this.#sent += parts.length;
    }
    if (this.#head > 4096 && this.#head * 2 > this.#queue.length) {
      this.#queue = this.#queue.slice(this.#head);
      this.#head = 0;
    }
    if (this.#pressured && this.buffered < this.pressureAt) this.#pressured = false;
    if (this.#ending && this.buffered === 0 && !this.#endSent) {
      const end =
        `{"jsonrpc":"2.0","method":${JSON.stringify(`${wire.backend}.end`)},"params":` +
        `{"subscription":${JSON.stringify(wire.subscription)},"seq":${this.#seq}}}`;
      if (wire.text(end)) this.#endSent = true;
    }
  }
}

/** What a transport that carries an input half answers to (the generated one does). */
export interface DuplexRpcClient {
  readonly carriesInput: true;
  call(method: string, params: unknown, input: InputHalf<unknown>): AsyncGenerator<PlexusStreamItem>;
}

function duplexOf(rpc: RpcClient, method: string): DuplexRpcClient {
  if ((rpc as Partial<DuplexRpcClient>).carriesInput !== true) {
    throw new TypeError(`${method} takes an input stream, and this RpcClient carries none (no \`carriesInput\`)`);
  }
  return rpc as unknown as DuplexRpcClient;
}

/** The call's items, its input half's errors typed, a dropped socket a DisconnectedError. */
async function* duplexItems<T>(half: InputHalf<T>, raw: AsyncGenerator<PlexusStreamItem>): AsyncGenerator<PlexusStreamItem> {
  if (half.aborted) {
    // aborted before it started: no call, no socket
    half.callEnded();
    await raw.return(undefined);
    throw half.abortReason;
  }
  try {
    for await (const item of raw) {
      if (item.type === 'error') {
        const e = half.errorFor(item as unknown as ErrorItem);
        if (e) throw e;
      }
      yield item;
    }
  } catch (e) {
    if (e instanceof InputError || half.aborted) throw half.aborted ? half.abortReason : e;
    if (half.disconnected || (e instanceof Error && e.name === 'TransportError')) {
      half.lost();
      if (half.disconnected) throw new DisconnectedError(half.disconnected);
    }
    throw e;
  } finally {
    // However the call ended — its answer, a refusal at open, a reader that
    // let go — the input half sends no more and its source is returned.
    half.callEnded();
  }
  if (half.aborted) throw half.abortReason;
}

/** Started now, not when first read: a push call sends as soon as it is open. */
function started(gen: AsyncGenerator<PlexusStreamItem>): AsyncGenerator<PlexusStreamItem> {
  const first = gen.next();
  first.catch(() => {});
  return (async function* () {
    let r = await first;
    try {
      while (!r.done) {
        yield r.value;
        r = await gen.next();
      }
    } finally {
      if (!r.done) await gen.return(undefined);
    }
  })();
}

/** The pull form: `source` is drained into the call as credit allows. */
export function pullInput<T>(
  rpc: RpcClient,
  method: string,
  params: unknown,
  binary: boolean,
  source: AsyncIterable<T> | Iterable<T>,
  options?: InputOptions,
): AsyncGenerator<PlexusStreamItem> {
  const duplex = duplexOf(rpc, method);
  const half = new InputHalf<T>(method, binary, options);
  void half.pullFrom(source);
  return duplexItems(half, duplex.call(method, params, half as InputHalf<unknown>));
}

/** The push form: an open call, its input written as it comes, its output read by `decode`. */
export function pushInput<T, O>(
  rpc: RpcClient,
  method: string,
  params: unknown,
  binary: boolean,
  options: InputOptions | undefined,
  decode: (stream: AsyncGenerator<PlexusStreamItem>) => O,
): DuplexCall<T, O> {
  const duplex = duplexOf(rpc, method);
  const half = new InputHalf<T>(method, binary, options);
  const output = decode(started(duplexItems(half, duplex.call(method, params, half as InputHalf<unknown>))));
  // An output no one awaits (an application listening on onDisconnect alone) is no unhandled rejection.
  if (output instanceof Promise) output.catch(() => {});
  return {
    input: half,
    output,
    onDisconnect: (handler) => half.onDisconnect(handler),
  };
}
