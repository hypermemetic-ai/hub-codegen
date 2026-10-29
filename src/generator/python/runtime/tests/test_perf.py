"""PYTHON-8 perf: the runtime alone, 10,000 items on one subscription over loopback.

    python -I -m pytest -m perf -s     # prints the measured numbers

The MockService serves from a thread with its own event loop, so the time is the
client's: the reader, the routing, the inbox, ``decode=dict``.
"""

from __future__ import annotations

import asyncio
import os
import subprocess
import sys
import threading
import time
import tracemalloc
from collections.abc import Iterator
from pathlib import Path

import pytest

from runtime.transport import Service

from .mock_service import DONE, MockService, data

pytestmark = pytest.mark.perf

N = 10_000
ITEM = {"type": "summary", "summary": {"id": "PYTHON-8", "title": "The Python client's runtime", "status": "Coding",
                                        "kind": "build", "epic": "PYTHON", "child_count": 0, "labels": []}}


class ThreadedMock:
    """A MockService on its own loop in a thread, so its sending is not timed."""

    def __init__(self) -> None:
        self.loop = asyncio.new_event_loop()
        self.service = MockService()
        self.thread = threading.Thread(target=self.loop.run_forever, daemon=True)

    def __enter__(self) -> MockService:
        self.thread.start()
        asyncio.run_coroutine_threadsafe(self.service.__aenter__(), self.loop).result(5)
        return self.service

    def __exit__(self, *exc: object) -> None:
        asyncio.run_coroutine_threadsafe(self.service.__aexit__(), self.loop).result(5)
        self.loop.call_soon_threadsafe(self.loop.stop)
        self.thread.join(5)


@pytest.fixture
def threaded() -> Iterator[MockService]:
    with ThreadedMock() as service:
        yield service


async def test_decode_10k_items(threaded: MockService) -> None:
    items = [data(ITEM) for _ in range(N)] + [DONE]
    threaded.expect_call("tickets.list").reply(items, blast=True)
    threaded.expect_call("tickets.list").reply(items, blast=True)
    async with Service(threaded.url, backend="menger") as service:
        # throughput: wall time around the async for, no tracing
        t0 = time.perf_counter()
        n = 0
        async for _ in service.call("tickets.list", {}, decode=dict):
            n += 1
        wall = time.perf_counter() - t0
        assert n == N
        # memory: tracemalloc peak at 1,000 and at 10,000 items
        tracemalloc.start()
        peaks: dict[int, int] = {}
        n = 0
        async for _ in service.call("tickets.list", {}, decode=dict):
            n += 1
            if n in (1_000, N):
                peaks[n] = tracemalloc.get_traced_memory()[1]
        tracemalloc.stop()
    rate = N / wall
    growth = peaks[N] - peaks[1_000]
    print(f"\nperf: {N} items in {wall * 1000:.1f} ms = {rate:,.0f} items/s ({wall / N * 1e6:.1f} µs/item); "
          f"tracemalloc peak @1k {peaks[1_000] / 1024:.0f} KiB, @10k {peaks[N] / 1024:.0f} KiB, growth {growth / 1024:.0f} KiB")
    assert rate >= 50_000, f"{rate:,.0f} items/s < 50,000"
    assert growth <= 256 * 1024, f"peak grew {growth / 1024:.0f} KiB between 1,000 and 10,000 items"


async def test_open_watch_memory(threaded: MockService) -> None:
    for _ in range(100):
        threaded.expect_call("tickets.watch").reply([data({"type": "watching"})])
    async with Service(threaded.url, backend="menger") as service:
        tracemalloc.start()
        base = tracemalloc.get_traced_memory()[0]
        streams = [service.call("tickets.watch", {}, decode=dict) for _ in range(100)]
        for s in streams:
            await s.__anext__()
        held = tracemalloc.get_traced_memory()[0] - base
        tracemalloc.stop()
        for s in streams:
            await s.aclose()
    print(f"\nperf: an open watch holds {held / 100:,.0f} B (100 open, one item read each)")
    assert held / 100 <= 16 * 1024


def test_import_time() -> None:
    """The runtime's own import cost, over the ``asyncio`` an async program has already paid; and cold, whole."""
    root = str(Path(__file__).resolve().parents[2])
    code = ("import time, sys; t0 = time.perf_counter(); import asyncio; t1 = time.perf_counter(); "
            "import runtime.transport, runtime.credentials, runtime.errors, runtime._decode; t2 = time.perf_counter(); "
            "print((t2 - t1) * 1000, (t2 - t0) * 1000, 'cryptography' in sys.modules, 'websockets' in sys.modules)")
    env = {**os.environ, "PYTHONPATH": root}
    own, cold = [], []
    for _ in range(7):
        out = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, check=True, env=env)
        a, b, crypto, ws = out.stdout.split()
        assert (crypto, ws) == ("False", "False")
        own.append(float(a))
        cold.append(float(b))
    own.sort()
    cold.sort()
    print(f"\nperf: import of the runtime over asyncio: median {own[3]:.1f} ms (min {own[0]:.1f}); "
          f"cold with asyncio: median {cold[3]:.1f} ms; websockets loads at the first connect")
    assert own[3] <= 40.0
