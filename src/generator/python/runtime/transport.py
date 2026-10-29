"""One socket, many calls: the transport of a generated Plexus RPC client (PYTHON-8).

    async with Hub("ws://127.0.0.1:4460", backend="menger",
                   credentials=KeyProof.from_file("~/.plexus/keys/builder.key", agent="builder")) as hub:
        async for ev in hub.call("tickets.list", {"filter": "in PYTHON", "summary": True}, decode=dict):
            print(ev["type"])
        posted = await hub.call("chats.post", {"chat": 6, "body": "hi"}, decode=dict).one()

The wire is today's: ``<backend>.call {method, params}`` is answered with a
subscription id; the call's items arrive as ``subscription`` notifications
(``params.result``) until ``done``; ``<backend>.call_unsub [id]`` ends a call
the caller stopped reading (synapse's frame); ``<backend>.respond`` answers a
``request`` item. One reader task routes every notification to its call's
bounded inbox; nothing new is sent that synapse does not send.

Embedded verbatim in every generated package by hub-codegen (PYTHON-4); it
knows no backend's methods. PYTHON-7 extends ``Hub._open``, ``Hub._send`` and
``MethodStream._control`` for a call's input half.
"""

from __future__ import annotations

import asyncio
import itertools
import json
from collections import deque
from collections.abc import Awaitable, Callable, Mapping
from dataclasses import dataclass
from types import TracebackType
from typing import TYPE_CHECKING, Any, Generic, Literal, TypeVar, overload

if TYPE_CHECKING:  # websockets itself is imported at the first connect: importing a client costs no socket library
    from websockets.asyncio.client import ClientConnection

from .credentials import Credentials, authorization
from .errors import (
    ConnectFailed,
    ConnectionLost,
    EmptyStream,
    MissingCredential,
    PlexusError,
    Unauthenticated,
    error_from_item,
    error_from_rpc,
)

__all__ = ["Hub", "MethodStream", "Page", "Progress", "Request", "Requirement", "RequestHook", "ProgressHook"]

E = TypeVar("E")
E_co = TypeVar("E_co", covariant=True)

_DROPPED_KEPT = 1024  # ids of calls the caller abandoned, kept so their late items are dropped, not buffered


@dataclass(frozen=True, slots=True)
class Page:
    """Where a paged read stopped: ``next`` cursor, ``remaining`` rows, ``first`` position."""

    next: str | None
    remaining: int
    first: int | None = None


@dataclass(frozen=True, slots=True)
class Progress:
    """A ``progress`` item: a message and, when given, a percentage (0–100)."""

    message: str
    percentage: float | None = None


@dataclass(frozen=True, slots=True)
class Request:
    """A ``request`` item: the hub asks the client something mid-call (bidirectional)."""

    request_id: str
    data: Any
    timeout_ms: int
    method: str
    subscription: int


@dataclass(frozen=True, slots=True)
class Requirement:
    """What a method needs before it is sent (a generated client's ``PREFLIGHT`` table)."""

    grant: str | None = None
    target: str | None = None
    credential: bool = False


RequestHook = Callable[[Request], Awaitable[Any]]
"""Answers a ``request`` item; its value is sent as ``response_data`` (None: ``{"type": "cancelled"}``)."""

ProgressHook = Callable[[Progress], Any]
"""Hears each ``progress`` item as the caller reads past it; may be sync or async."""


@dataclass(frozen=True, slots=True)
class _End:
    reason: str | None
    closed_by_caller: bool


class _Inbox:
    """One call's bounded queue of raw items, filled by the reader, emptied by the stream."""

    __slots__ = ("method", "items", "maxsize", "end", "dropped", "_getter", "_putter")

    def __init__(self, maxsize: int, method: str) -> None:
        self.method = method
        self.items: deque[Any] = deque()
        self.maxsize = maxsize
        self.end: _End | None = None
        self.dropped = False
        self._getter: asyncio.Future[None] | None = None
        self._putter: asyncio.Future[None] | None = None

    def push(self, item: Any) -> None:
        self.items.append(item)
        g = self._getter
        if g is not None:
            self._getter = None
            if not g.done():
                g.set_result(None)

    async def put(self, item: Any) -> None:
        """Append, waiting for room: a stalled reader of this call stalls the socket (backpressure)."""
        while len(self.items) >= self.maxsize and self.end is None and not self.dropped:
            self._putter = asyncio.get_running_loop().create_future()
            await self._putter
        if not self.dropped:
            self.push(item)

    async def get(self) -> Any:
        while not self.items:
            if self.end is not None:
                raise _Ended(self.end)
            self._getter = asyncio.get_running_loop().create_future()
            await self._getter
        return self.take()

    def take(self) -> Any:
        item = self.items.popleft()
        p = self._putter
        if p is not None:
            self._putter = None
            if not p.done():
                p.set_result(None)
        return item

    def finish(self, end: _End) -> None:
        self.end = end
        self._wake()

    def drop(self) -> None:
        self.dropped = True
        self.items.clear()
        self._wake()

    def _wake(self) -> None:
        for f in (self._getter, self._putter):
            if f is not None and not f.done():
                f.set_result(None)
        self._getter = self._putter = None


class _Ended(Exception):
    def __init__(self, end: _End) -> None:
        self.end = end


def _page(d: Mapping[str, Any]) -> Page:
    nxt, rem, first = d.get("next"), d.get("remaining"), d.get("first")
    return Page(next=None if nxt is None else str(nxt), remaining=rem if isinstance(rem, int) else 0,
                first=first if isinstance(first, int) else None)


class MethodStream(Generic[E_co]):
    """One call's items as an async iterator — lazy: nothing is sent until it is first read.

    ``async for ev in stream`` yields each ``data`` item's ``decode(content)``
    and ends at ``done``; an ``error`` item, the call's last, raises its
    ``PlexusError`` (or is yielded, and the stream ends, under ``errors="yield"``); ``page`` and ``progress`` items are
    kept on ``.page`` / ``.progress``, not yielded. Leaving early — ``break``,
    ``aclose()``, or ``async with`` — unsubscribes. Hold the stream in a
    variable and ``break``, and the unsubscribe waits for the stream to be
    collected: use ``async with`` (or ``aclose()``) for a watch.
    """

    __slots__ = ("_hub", "method", "_params", "_decode", "_yield_errors", "_inbox", "_sub", "_state",
                 "items_seen", "page", "progress", "__weakref__")

    def __init__(self, hub: Hub, method: str, params: Mapping[str, object], decode: Callable[[Any], E_co],
                 yield_errors: bool) -> None:
        self._hub = hub
        self.method = method
        self._params = params
        self._decode = decode
        self._yield_errors = yield_errors
        self._inbox = _Inbox(hub._max_queue, method)
        self._sub: int | None = None
        self._state = 0  # 0 not sent · 1 open · 2 finished
        self.items_seen = 0
        self.page: Page | None = None
        self.progress: Progress | None = None

    @property
    def subscription(self) -> int | None:
        """The call's subscription id, once the hub has answered it."""
        return self._sub

    @property
    def buffered(self) -> int:
        """Items received and not yet read (at most the Hub's ``max_queue``)."""
        return len(self._inbox.items)

    def __aiter__(self) -> MethodStream[E_co]:
        return self

    async def __anext__(self) -> E_co:
        if self._state != 1:
            if self._state == 2:
                raise StopAsyncIteration
            await self._start()
        inbox = self._inbox
        while True:
            if inbox.items:
                item = inbox.take()
            else:
                try:
                    item = await inbox.get()
                except _Ended as e:
                    self._state = 2
                    raise ConnectionLost(self.method, self._sub, self.items_seen,
                                         closed_by_caller=e.end.closed_by_caller, reason=e.end.reason) from None
            kind = item.get("type") if type(item) is dict else None
            if kind == "data":
                content = item.get("content")
                if type(content) is dict and content.get("type") == "page" and "remaining" in content:
                    self.page = _page(content)
                    continue
                self.items_seen += 1
                return self._decode(content)
            if kind == "done":
                self._finish()
                raise StopAsyncIteration
            if kind == "error":
                # an error is the call's last item: a gate's refusal has no `done` after it (a trailing
                # `done`, as plexus-core's guidance stream sends, is dropped with the call)
                err = error_from_item(self.method, item)
                self._finish(drop_late=True)
                if self._yield_errors:
                    return err  # type: ignore[return-value]  # E_co is E | PlexusError under errors="yield"
                raise err
            if kind == "progress":
                p = Progress(message=str(item.get("message", "")), percentage=item.get("percentage"))
                self.progress = p
                hook = self._hub._on_progress
                if hook is not None:
                    r = hook(p)
                    if isinstance(r, Awaitable):
                        await r
                continue
            if kind == "page":
                self.page = _page(item)
                continue
            self._control(item)

    def _control(self, item: Any) -> None:
        """A control item that is not data, done, error, progress or page (``credit``, ``ack``,
        ``input_closed``, …): ignored here; PYTHON-7's input half overrides it."""

    async def _start(self) -> None:
        self._state = 1
        try:
            self._sub = await self._hub._open(self.method, self._params, self._inbox)
        except BaseException:
            self._state = 2
            raise

    def _finish(self, drop_late: bool = False) -> None:
        self._state = 2
        if self._sub is not None:
            self._hub._forget(self._sub, drop_late=drop_late)

    async def one(self) -> E_co:
        """The first data item; the rest of the call is cancelled. ``EmptyStream`` if there was none."""
        try:
            async for x in self:
                return x
            raise EmptyStream(self.method, "the call ended with no item")
        finally:
            await self.aclose()

    async def collect(self) -> list[E_co]:
        """Every data item, until ``done``."""
        return [x async for x in self]

    async def aclose(self) -> None:
        """Stop reading: the call is unsubscribed (once); later items for it are dropped."""
        if self._state == 1 and self._sub is not None:
            self._state = 2
            await self._hub._unsubscribe(self._sub)
        else:
            self._state = 2

    async def __aenter__(self) -> MethodStream[E_co]:
        return self

    async def __aexit__(self, et: type[BaseException] | None, e: BaseException | None,
                        tb: TracebackType | None) -> None:
        await self.aclose()

    def __del__(self) -> None:
        if self._state == 1 and self._sub is not None:
            self._hub._abandon(self._sub)


class Hub:
    """One WebSocket to a Plexus hub, many calls on it. ``async with Hub(...)``, or ``await hub.connect()``.

    ``credentials`` are sent as ``Authorization: Bearer …`` at the upgrade.
    ``preflight`` (a generated client's table) refuses a method that needs a
    credential when there is none, before anything is sent. ``max_queue``
    bounds each call's unread items; a consumer that stops reading stalls the
    socket. A dropped socket ends every open call with ``ConnectionLost``;
    there is no reconnect — ``connect()`` again for a new socket.
    """

    def __init__(self, url: str, *, backend: str, credentials: Credentials | None = None,
                 on_request: RequestHook | None = None, on_progress: ProgressHook | None = None,
                 preflight: Mapping[str, Requirement] = {}, connect_timeout: float = 10.0,
                 max_queue: int = 4096) -> None:
        self.url = url
        self.backend = backend
        self._credentials = credentials
        self._on_request = on_request
        self._on_progress = on_progress
        self._preflight = preflight
        self._connect_timeout = connect_timeout
        self._max_queue = max(1, max_queue)
        self._ids = itertools.count(1)
        self._ws: ClientConnection | None = None
        self._reader: asyncio.Task[None] | None = None
        self._loop: asyncio.AbstractEventLoop | None = None
        self._wlock = asyncio.Lock()
        self._pending: dict[int, tuple[asyncio.Future[int], _Inbox]] = {}
        self._subs: dict[int, _Inbox] = {}
        self._early: dict[int, list[Any]] = {}
        self._dropped: dict[int, None] = {}
        self._tasks: set[asyncio.Task[None]] = set()
        self._closing = False
        self._lost: _End | None = None

    # ── the socket ─────────────────────────────────────────────────────────

    async def connect(self) -> None:
        """Open the socket and sign in; a no-op when it is open."""
        if self._ws is not None:
            return
        from websockets.asyncio.client import connect
        from websockets.exceptions import InvalidHandshake, InvalidStatus, InvalidURI

        headers: dict[str, str] = {}
        if self._credentials is not None:
            headers["Authorization"] = await authorization(self._credentials, self.url.rstrip("/"))
        try:
            ws = await connect(self.url, additional_headers=headers, max_size=None,
                               open_timeout=self._connect_timeout)
        except InvalidStatus as e:
            status = e.response.status_code
            if status in (401, 403):
                raise Unauthenticated(None, f"{self.url} refused the sign-in ({status}): is the credential "
                                            f"granted, and is this URL the hub's own?", url=self.url) from None
            raise ConnectFailed(self.url, f"answered {status}, not a WebSocket upgrade", status=status) from None
        except (OSError, TimeoutError, InvalidHandshake, InvalidURI) as e:
            raise ConnectFailed(self.url, str(e) or type(e).__name__) from None
        self._ws = ws
        self._loop = asyncio.get_running_loop()
        self._closing = False
        self._lost = None
        self._reader = asyncio.create_task(self._read(ws))

    async def close(self) -> None:
        """End every open call (``ConnectionLost(closed_by_caller=True)``) and close the socket; idempotent."""
        ws, reader = self._ws, self._reader
        if ws is None and reader is None:
            return
        self._closing = True
        if ws is not None:
            await ws.close()
        if reader is not None:
            await reader
        self._reader = None
        for t in list(self._tasks):
            t.cancel()

    async def __aenter__(self) -> Hub:
        await self.connect()
        return self

    async def __aexit__(self, et: type[BaseException] | None, e: BaseException | None,
                        tb: TracebackType | None) -> None:
        await self.close()

    # ── calls ──────────────────────────────────────────────────────────────

    @overload
    def call(self, method: str, params: Mapping[str, object], decode: Callable[[Any], E], *,
             errors: Literal["raise"] = "raise") -> MethodStream[E]: ...

    @overload
    def call(self, method: str, params: Mapping[str, object], decode: Callable[[Any], E], *,
             errors: Literal["yield"]) -> MethodStream[E | PlexusError]: ...

    @overload
    def call(self, method: str, params: Mapping[str, object], decode: Callable[[Any], E], *,
             errors: Literal["raise", "yield"]) -> MethodStream[E | PlexusError]: ...

    def call(self, method: str, params: Mapping[str, object], decode: Callable[[Any], E], *,
             errors: Literal["raise", "yield"] = "raise") -> MethodStream[E] | MethodStream[E | PlexusError]:
        """``method``'s items as a ``MethodStream``, each ``decode(content)``; nothing is sent until it is read.

        ``errors="raise"``: an error item raises its ``PlexusError``;
        ``"yield"``: it is yielded as the stream's last item.
        """
        need = self._preflight.get(method)
        if need is not None and need.credential and self._credentials is None:
            raise MissingCredential(method, need.grant)
        return MethodStream(self, method, params, decode, errors == "yield")

    async def _open(self, method: str, params: Mapping[str, object], inbox: _Inbox) -> int:
        """Send ``<backend>.call`` and return its subscription id; ``inbox`` gets its items (PYTHON-7's seam)."""
        if self._ws is None:
            if self._lost is not None:
                raise ConnectionLost(method, None, 0, closed_by_caller=self._lost.closed_by_caller,
                                     reason=self._lost.reason)
            await self.connect()
        rid = next(self._ids)
        fut: asyncio.Future[int] = asyncio.get_running_loop().create_future()
        self._pending[rid] = (fut, inbox)
        try:
            await self._send({"jsonrpc": "2.0", "id": rid, "method": f"{self.backend}.call",
                              "params": {"method": method, "params": dict(params)}})
        except BaseException:
            self._pending.pop(rid, None)
            raise
        return await fut

    async def _send(self, frame: Mapping[str, Any]) -> None:
        """Write one frame, under the one writer lock (PYTHON-7's seam)."""
        ws = self._ws
        if ws is None:
            raise ConnectionLost(str(frame.get("method")), None, 0,
                                 closed_by_caller=bool(self._lost and self._lost.closed_by_caller))
        from websockets.exceptions import ConnectionClosed

        text = json.dumps(frame)
        try:
            async with self._wlock:
                await ws.send(text)
        except ConnectionClosed as e:
            raise ConnectionLost(str(frame.get("method")), None, 0, reason=str(e)) from None

    def _forget(self, sub: int, *, drop_late: bool) -> None:
        inbox = self._subs.pop(sub, None)
        if inbox is not None:
            inbox.drop()
        self._early.pop(sub, None)
        if drop_late:
            self._dropped[sub] = None
            if len(self._dropped) > _DROPPED_KEPT:
                del self._dropped[next(iter(self._dropped))]

    def _unsub_frame(self, sub: int) -> dict[str, Any]:
        return {"jsonrpc": "2.0", "id": next(self._ids), "method": f"{self.backend}.call_unsub", "params": [sub]}

    async def _unsubscribe(self, sub: int) -> None:
        self._forget(sub, drop_late=True)
        if self._ws is not None:
            try:
                await self._send(self._unsub_frame(sub))
            except PlexusError:
                pass

    def _abandon(self, sub: int) -> None:
        """A stream collected while open (``break`` out of ``async for``): unsubscribe it from the loop."""
        loop = self._loop
        if loop is None or loop.is_closed():
            return
        try:
            loop.call_soon_threadsafe(self._spawn_unsubscribe, sub)
        except RuntimeError:
            pass

    def _spawn_unsubscribe(self, sub: int) -> None:
        self._spawn(self._unsubscribe(sub))

    def _spawn(self, coro: Awaitable[None]) -> None:
        task: asyncio.Task[None] = asyncio.ensure_future(coro)
        self._tasks.add(task)
        task.add_done_callback(self._tasks.discard)

    # ── the reader: the only task that reads the socket ────────────────────

    async def _read(self, ws: ClientConnection) -> None:
        from websockets.exceptions import ConnectionClosed

        reason: str | None = None
        subs, early, dropped = self._subs, self._early, self._dropped
        try:
            async for raw in ws:
                try:
                    msg = json.loads(raw)
                except ValueError:
                    continue
                if type(msg) is not dict:
                    continue
                params = msg.get("params")
                if type(params) is dict and "subscription" in params:
                    sub = params["subscription"]
                    item = params.get("result")
                    inbox = subs.get(sub)
                    if inbox is not None:
                        await self._deliver(sub, inbox, item)
                    elif sub in dropped:
                        if type(item) is dict and item.get("type") in ("done", "error"):
                            del dropped[sub]
                    else:
                        early.setdefault(sub, []).append(item)
                elif "id" in msg and "method" not in msg:
                    await self._answered(msg)
        except ConnectionClosed as e:
            reason = None if self._closing else str(e)
        except Exception as e:  # never leave a call waiting on a dead reader
            reason = f"{type(e).__name__}: {e}"
        finally:
            self._lost_socket(reason)

    async def _deliver(self, sub: int, inbox: _Inbox, item: Any) -> None:
        if type(item) is dict and item.get("type") == "request":
            self._spawn(self._answer(sub, inbox.method, item))
        elif len(inbox.items) < inbox.maxsize:
            inbox.push(item)
        else:
            await inbox.put(item)

    async def _answered(self, msg: dict[str, Any]) -> None:
        entry = self._pending.pop(msg["id"], None)
        if entry is None:
            return  # the answer to an unsubscribe or a respond: nobody waits on it
        fut, inbox = entry
        err = msg.get("error")
        if err is not None:
            if not fut.done():
                fut.set_exception(error_from_rpc(inbox.method, err if isinstance(err, dict) else {"message": str(err)}))
            return
        result = msg.get("result")
        sub: Any = result.get("subscription") if isinstance(result, dict) else result
        if fut.done():  # the caller gave up while the call was in flight
            self._dropped[sub] = None
            self._early.pop(sub, None)
            if self._ws is not None:
                self._spawn(self._send(self._unsub_frame(sub)))
            return
        self._subs[sub] = inbox
        fut.set_result(sub)
        for item in self._early.pop(sub, ()):
            await self._deliver(sub, inbox, item)

    async def _answer(self, sub: int, method: str, item: Mapping[str, Any]) -> None:
        raw_timeout = item.get("timeoutMs")
        req = Request(request_id=str(item.get("requestId", "")), data=item.get("requestData"),
                      timeout_ms=raw_timeout if isinstance(raw_timeout, int) else 0, method=method, subscription=sub)
        answer: Any = None
        hook = self._on_request
        if hook is not None:
            try:
                if req.timeout_ms > 0:
                    answer = await asyncio.wait_for(hook(req), req.timeout_ms / 1000)
                else:
                    answer = await hook(req)
            except Exception:  # a failing or late hook answers cancelled, as the TypeScript client does
                answer = None
        if answer is None:
            answer = {"type": "cancelled"}
        try:
            await self._send({"jsonrpc": "2.0", "id": next(self._ids), "method": f"{self.backend}.respond",
                              "params": {"request_id": req.request_id, "response_data": answer}})
        except PlexusError:
            pass

    def _lost_socket(self, reason: str | None) -> None:
        end = _End(reason, self._closing)
        self._ws = None
        self._lost = end
        for inbox in self._subs.values():
            inbox.finish(end)
        self._subs.clear()
        for fut, inbox in self._pending.values():
            if not fut.done():
                fut.set_exception(ConnectionLost(inbox.method, None, 0, closed_by_caller=end.closed_by_caller,
                                                 reason=reason))
        self._pending.clear()
        self._early.clear()
        self._dropped.clear()
