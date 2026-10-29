"""PYTHON-8 live: the runtime against a throwaway menger (``-m live``).

Needs ``PLEXUS_URL`` (``ws://127.0.0.1:4486``) and ``PLEXUS_KEY_FILE`` — a key
file (0600) the server's ``--admin`` names; ``PLEXUS_AGENT`` (default
``python-live``) is the agent it signs in as. Never a production hub: the test
files a ticket and grants a second, throwaway key ``editor`` on ``*``.
"""

from __future__ import annotations

import asyncio
import base64
import os
import secrets
import time
from pathlib import Path
from typing import Any

import pytest

from runtime.credentials import KeyProof
from runtime.errors import Forbidden
from runtime.transport import Hub

pytestmark = [
    pytest.mark.live,
    pytest.mark.skipif(not (os.environ.get("PLEXUS_URL") and os.environ.get("PLEXUS_KEY_FILE")),
                       reason="needs PLEXUS_URL and PLEXUS_KEY_FILE of a throwaway hub"),
]


def _admin() -> tuple[str, KeyProof]:
    return os.environ["PLEXUS_URL"], KeyProof.from_file(os.environ["PLEXUS_KEY_FILE"],
                                                         agent=os.environ.get("PLEXUS_AGENT", "python-live"))


def _editor_key(tmp: Path) -> KeyProof:
    path = tmp / "editor.key"
    path.write_text(base64.b64encode(secrets.token_bytes(32)).decode())
    path.chmod(0o600)
    return KeyProof.from_file(path, agent="python-editor")


async def _scope(hub: Hub, code: str) -> str:
    created = await hub.call("tickets.create", {"epic": code, "title": "PYTHON-8 live", "kind": "scope"}, decode=dict).one()
    assert created.get("type") == "created", created
    ticket_id: str = created["ticket"]["id"]
    return ticket_id


async def test_live_list_and_forbidden(tmp_path: Path) -> None:
    url, admin = _admin()
    code = "PYL" + "".join(secrets.choice("ABCDEFGHJKLMNPQRSTUVWXYZ") for _ in range(3))
    t0 = time.perf_counter()
    async with Hub(url, backend="menger", credentials=admin) as hub:
        first = await hub.call("tickets.health", {}, decode=dict).one()
        first_item_ms = (time.perf_counter() - t0) * 1000
        assert first
        ticket_id = await _scope(hub, code)
        listed = hub.call("tickets.list", {"filter": f"in {code}", "summary": True}, decode=dict)
        events = await listed.collect()
        assert [e["summary"]["id"] for e in events if e.get("type") == "summary"] == [ticket_id]
        editor = _editor_key(tmp_path)
        granted = await hub.call("members.grant", {"principal": editor.principal, "scope": "*", "role": "editor"},
                                 decode=dict).collect()
        assert all(g.get("type") != "error" for g in granted), granted
    print(f"\nlive: connect + key proof + first item {first_item_ms:.1f} ms; page {listed.page}")
    async with Hub(url, backend="menger", credentials=editor) as hub:
        with pytest.raises(Forbidden) as ei:
            await hub.call("tickets.delete", {"ids": [ticket_id]}, decode=dict).collect()
    print(f"live: {ei.value}")
    # AUTH-7: every method is its own grant; the roles that hold it are named beside it
    assert ei.value.grant == "tickets.delete"
    assert ei.value.roles == ["admin"]
    assert ei.value.scope == ticket_id
    assert ei.value.held_on == []  # where the caller holds tickets.delete: an editor, nowhere
    assert ei.value.method == "tickets.delete"


async def test_live_cancel_watch() -> None:
    url, admin = _admin()
    code = "PYW" + "".join(secrets.choice("ABCDEFGHJKLMNPQRSTUVWXYZ") for _ in range(3))
    async with Hub(url, backend="menger", credentials=admin) as hub:
        first: list[Any] = []

        async def watch_one() -> None:
            async for ev in hub.call("tickets.watch", {}, decode=dict):
                first.append(ev)
                break

        task = asyncio.create_task(watch_one())
        await asyncio.sleep(0.3)
        if not task.done():  # a watch that says nothing until a change: make one
            await _scope(hub, code)
        await asyncio.wait_for(task, 10)
        assert first
        # the socket is not wedged by the cancel: a write and a read still answer
        ticket_id = await asyncio.wait_for(_scope(hub, code + "Z"), 10)
        got = await asyncio.wait_for(hub.call("tickets.get", {"id": ticket_id}, decode=dict).one(), 10)
        assert got["ticket"]["id"] == ticket_id
        assert hub._subs == {}
