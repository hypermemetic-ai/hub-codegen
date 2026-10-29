"""MockService: a scripted Plexus service for the runtime's tests (PYTHON-8), and for
PYTHON-4's generated-client tests and PYTHON-7's input tests.

A ``websockets`` server on ``127.0.0.1:0`` that speaks today's wire:
``<backend>.call {method, params}`` answered with a subscription id, then
``{"method": "<backend>.call", "params": {"subscription": id, "result": item}}``
notifications, ``<backend>.call_unsub [id]`` and ``<backend>.respond``
acknowledged. Every frame a client sends is kept in ``recorded``; every
upgrade's headers in ``upgrade_headers``.

    async with MockService() as mock:
        exp = mock.expect_call("tickets.get").reply([data({"type": "ticket"}), DONE])
        async with Service(mock.url, backend="menger") as service:
            ...
        await exp.push(data(...))          # more items for that call, later
        await mock.close_clients()         # drop every socket
"""

from __future__ import annotations

import asyncio
import json
from http import HTTPStatus
from typing import Any

from websockets.asyncio.server import Server, ServerConnection, serve
from websockets.datastructures import Headers
from websockets.exceptions import ConnectionClosed
from websockets.http11 import Request, Response

Frame = dict[str, Any]

DONE: Frame = {"type": "done", "metadata": {"provenance": ["mock"], "plexus_hash": "0", "timestamp": 0}}


def data(content: Any, content_type: str = "mock.event") -> Frame:
    """A ``data`` stream item carrying ``content``."""
    return {"type": "data", "metadata": {"provenance": ["mock"], "plexus_hash": "0", "timestamp": 0},
            "content_type": content_type, "content": content}


def error_item(message: str, **fields: Any) -> Frame:
    """An ``error`` stream item; ``fields`` adds ``error_kind``, ``grant`` and the rest."""
    item: Frame = {"type": "error", "metadata": {"provenance": ["mock"], "plexus_hash": "0", "timestamp": 0},
                   "message": message, "recoverable": False}
    item.update(fields)
    return item


def _notification(backend: str, sub: int, item: Frame) -> str:
    # menger names a notification by its call's method (`menger.call`); the client routes by `subscription`
    return json.dumps({"jsonrpc": "2.0", "method": f"{backend}.call",
                       "params": {"subscription": sub, "result": item}})


def _frame(payload: bytes) -> bytes:
    """One unmasked, final text frame (RFC 6455 §5.2), as a server writes it."""
    n = len(payload)
    if n < 126:
        head = bytes([0x81, n])
    elif n < 1 << 16:
        head = bytes([0x81, 126]) + n.to_bytes(2, "big")
    else:
        head = bytes([0x81, 127]) + n.to_bytes(8, "big")
    return head + payload


class Expectation:
    """One expected ``<backend>.call`` of ``method``, and what the service answers."""

    def __init__(self, service: MockService, method: str, sub: int) -> None:
        self.method = method
        self.sub = sub
        self.frame: Frame | None = None
        self._service = service
        self._encoded: list[str] = []
        self._early = 0
        self._close_after = False
        self._blast: bytes | None = None
        self._rpc_error: Frame | None = None
        self._conn: ServerConnection | None = None
        self._arrived = asyncio.Event()

    def reply(self, items: list[Frame], *, early: int = 0, close_after: bool = False,
              blast: bool = False) -> Expectation:
        """Answer the call with its subscription id, then ``items`` in order.

        ``early``: send that many items *before* the response. ``close_after``:
        close the socket once every item is sent. Leave ``DONE`` out to keep
        the call open and ``push`` more later. ``blast``: the items go as one
        pre-serialized write of WebSocket frames, so the service's own cost is
        out of a client's measurement (the perf tests).
        """
        self._encoded = [_notification(self._service.backend, self.sub, it) for it in items]
        self._early = early
        self._close_after = close_after
        self._blast = b"".join(_frame(raw.encode()) for raw in self._encoded[early:]) if blast else None
        return self

    def error(self, code: int, message: str, data: Any = None) -> Expectation:
        """Answer the call with a JSON-RPC error instead of a subscription."""
        err: Frame = {"code": code, "message": message}
        if data is not None:
            err["data"] = data
        self._rpc_error = err
        return self

    async def arrived(self, timeout: float = 5.0) -> Frame:
        """The call's frame, once it has arrived and been answered."""
        await asyncio.wait_for(self._arrived.wait(), timeout)
        assert self.frame is not None
        return self.frame

    async def push(self, *items: Frame) -> None:
        """Send more items for this call's subscription."""
        await self.arrived()
        assert self._conn is not None
        for it in items:
            await self._conn.send(_notification(self._service.backend, self.sub, it))

    async def _serve(self, conn: ServerConnection, frame: Frame) -> None:
        self.frame = frame
        self._conn = conn
        if self._rpc_error is not None:
            await conn.send(json.dumps({"jsonrpc": "2.0", "id": frame["id"], "error": self._rpc_error}))
            self._arrived.set()
            return
        for raw in self._encoded[: self._early]:
            await conn.send(raw)
        await conn.send(json.dumps({"jsonrpc": "2.0", "id": frame["id"], "result": self.sub}))
        self._arrived.set()
        if self._blast is not None:
            conn.transport.write(self._blast)
        else:
            for raw in self._encoded[self._early:]:
                await conn.send(raw)
        if self._close_after:
            await conn.close()


class MockService:
    """A scripted service; ``async with`` starts it on a free loopback port."""

    def __init__(self, backend: str = "menger") -> None:
        self.backend = backend
        self.recorded: list[Frame] = []
        self.upgrade_headers: list[Headers] = []
        self.connections: list[ServerConnection] = []
        self._expect: list[Expectation] = []
        self._next_sub = 100
        self._reject: int | None = None
        self._server: Server | None = None
        self._tasks: set[asyncio.Task[None]] = set()
        self.url = ""

    async def __aenter__(self) -> MockService:
        self._server = await serve(self._handler, "127.0.0.1", 0, process_request=self._process_request,
                                   max_size=None, compression=None)  # a Plexus service (jsonrpsee) offers no deflate
        port = next(iter(self._server.sockets)).getsockname()[1]
        self.url = f"ws://127.0.0.1:{port}"
        return self

    async def __aexit__(self, *exc: object) -> None:
        for t in list(self._tasks):
            t.cancel()
        if self._server is not None:
            self._server.close()
            await self._server.wait_closed()

    # ── scripting ───────────────────────────────────────────────────────────

    def expect_call(self, method: str) -> Expectation:
        """Expect one ``<backend>.call`` of ``method`` (first in, first answered)."""
        self._next_sub += 1
        exp = Expectation(self, method, self._next_sub)
        self._expect.append(exp)
        return exp

    def reject_upgrade(self, status: int = 401) -> None:
        """Refuse every upgrade from now on with ``status``."""
        self._reject = status

    def calls(self) -> list[Frame]:
        """The ``<backend>.call`` frames received, in order."""
        return [f for f in self.recorded if f.get("method") == f"{self.backend}.call"]

    def frames(self, method: str) -> list[Frame]:
        """The frames received whose JSON-RPC method is ``method``."""
        return [f for f in self.recorded if f.get("method") == method]

    async def wait_for_frame(self, method: str, count: int = 1, timeout: float = 5.0) -> list[Frame]:
        """Wait until ``count`` frames of JSON-RPC ``method`` have arrived."""
        loop = asyncio.get_running_loop()
        end = loop.time() + timeout
        while len(self.frames(method)) < count:
            if loop.time() > end:
                raise AssertionError(f"no {count} × {method} within {timeout}s; recorded: {self.recorded}")
            await asyncio.sleep(0.005)
        return self.frames(method)

    async def close_clients(self, *, abort: bool = False) -> None:
        """Drop every client socket: a close frame, or ``abort`` the TCP connection."""
        for conn in list(self.connections):
            if abort:
                conn.transport.abort()
            else:
                await conn.close()

    # ── the server ──────────────────────────────────────────────────────────

    def _process_request(self, conn: ServerConnection, request: Request) -> Response | None:
        self.upgrade_headers.append(request.headers)
        if self._reject is not None:
            return conn.respond(HTTPStatus(self._reject), "refused by MockService\n")
        return None

    async def _handler(self, conn: ServerConnection) -> None:
        self.connections.append(conn)
        try:
            async for raw in conn:
                frame: Frame = json.loads(raw)
                self.recorded.append(frame)
                await self._dispatch(conn, frame)
        except ConnectionClosed:
            pass

    async def _dispatch(self, conn: ServerConnection, frame: Frame) -> None:
        method = frame.get("method")
        if method == f"{self.backend}.call":
            want = frame.get("params", {}).get("method")
            exp = next((e for e in self._expect if e.method == want and e.frame is None), None)
            if exp is None:
                await conn.send(json.dumps({"jsonrpc": "2.0", "id": frame.get("id"),
                                            "error": {"code": -32601, "message": f"MockService: no call of {want} expected"}}))
                return
            exp.frame = frame  # claimed now, so a second call of the method takes the next one
            task = asyncio.create_task(exp._serve(conn, frame))
            self._tasks.add(task)
            task.add_done_callback(self._tasks.discard)
        elif "id" in frame:
            await conn.send(json.dumps({"jsonrpc": "2.0", "id": frame["id"], "result": True}))
