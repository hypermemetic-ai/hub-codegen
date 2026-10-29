"""PYTHON-8: Hub and MethodStream on a scripted mock hub."""

from __future__ import annotations

import asyncio
import gc
from typing import Any

import pytest

from runtime.errors import (
    ConnectionLost,
    Forbidden,
    InvalidParams,
    MissingCredential,
    PlexusError,
    RpcError,
    StreamError,
    TooManyCalls,
    Unauthenticated,
)
from runtime.transport import Hub, Page, Progress, Request, Requirement

from .mock_hub import DONE, MockHub, data, error_item

FORBIDDEN = error_item(
    "grant `admin` on `*` is required; you hold editor on `*`",
    code="-32003",
    error_kind="forbidden",
    grant="admin",
    scope="*",
    roles=["admin"],
    held_on=["editor@*"],
)


async def test_call_frame_shape(mock: MockHub) -> None:
    mock.expect_call("tickets.get").reply([data({"type": "ticket"}), DONE])
    async with Hub(mock.url, backend="menger") as hub:
        got = await hub.call("tickets.get", {"id": "X-1"}, decode=dict).collect()
    assert mock.calls()[0] == {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "menger.call",
        "params": {"method": "tickets.get", "params": {"id": "X-1"}},
    }
    assert got == [{"type": "ticket"}]


async def test_stream_is_lazy(mock: MockHub) -> None:
    mock.expect_call("tickets.get").reply([data({"n": 1}), DONE])
    async with Hub(mock.url, backend="menger") as hub:
        stream = hub.call("tickets.get", {"id": "X-1"}, decode=dict)
        await asyncio.sleep(0.05)
        assert mock.recorded == []
        assert await stream.__anext__() == {"n": 1}
        assert len(mock.calls()) == 1
        assert stream.method == "tickets.get"


async def test_items_routed_by_subscription(mock: MockHub) -> None:
    ea = mock.expect_call("a.watch").reply([])
    eb = mock.expect_call("b.watch").reply([])
    async with Hub(mock.url, backend="menger") as hub:
        ta = asyncio.create_task(hub.call("a.watch", {}, decode=dict).collect())
        tb = asyncio.create_task(hub.call("b.watch", {}, decode=dict).collect())
        await ea.arrived()
        await eb.arrived()
        await ea.push(data({"a": 1}))
        await eb.push(data({"b": 1}))
        await ea.push(data({"a": 2}))
        await eb.push(data({"b": 2}), DONE)
        await ea.push(data({"a": 3}), DONE)
        assert await ta == [{"a": 1}, {"a": 2}, {"a": 3}]
        assert await tb == [{"b": 1}, {"b": 2}]
    assert [f["id"] for f in mock.calls()] in ([1, 2], [2, 1])


async def test_notification_before_response_is_kept(mock: MockHub) -> None:
    mock.expect_call("tickets.list").reply([data({"n": 1}), data({"n": 2}), DONE], early=1)
    async with Hub(mock.url, backend="menger") as hub:
        assert await hub.call("tickets.list", {}, decode=dict).collect() == [{"n": 1}, {"n": 2}]


async def test_done_ends_iteration(mock: MockHub) -> None:
    mock.expect_call("tickets.list").reply([data({"n": 1}), data({"n": 2}), DONE])
    async with Hub(mock.url, backend="menger") as hub:
        seen = [ev async for ev in hub.call("tickets.list", {}, decode=dict)]
        assert seen == [{"n": 1}, {"n": 2}]
        assert hub._subs == {}


async def test_one_and_collect(mock: MockHub) -> None:
    one = mock.expect_call("tickets.watch").reply([data({"n": 1}), data({"n": 2})])  # no done: open
    mock.expect_call("tickets.list").reply([data({"n": 1}), data({"n": 2}), DONE])
    async with Hub(mock.url, backend="menger") as hub:
        assert await hub.call("tickets.watch", {}, decode=dict).one() == {"n": 1}
        unsub = await mock.wait_for_frame("menger.call_unsub")
        assert unsub[0]["params"] == [one.sub]
        assert await hub.call("tickets.list", {}, decode=dict).collect() == [{"n": 1}, {"n": 2}]


async def test_error_item_raises_typed(mock: MockHub) -> None:
    mock.expect_call("tickets.delete").reply([data({"n": 1}), FORBIDDEN])
    mock.expect_call("tickets.delete").reply([data({"n": 1}), FORBIDDEN, DONE])
    async with Hub(mock.url, backend="menger") as hub:
        seen: list[Any] = []
        with pytest.raises(Forbidden) as ei:
            async for ev in hub.call("tickets.delete", {"ids": ["X-1"]}, decode=dict):
                seen.append(ev)
        assert seen == [{"n": 1}]
        e = ei.value
        assert (e.grant, e.scope, e.roles, e.held_on, e.method) == ("admin", "*", ["admin"], ["editor@*"], "tickets.delete")
        assert e.kind == "forbidden"
        assert str(e) == (
            "tickets.delete — grant `admin` on `*` is required; you hold editor on `*`\n"
            "  error_kind=forbidden grant=admin scope=* held_on=[editor@*]"
        )
        yielded = await hub.call("tickets.delete", {"ids": ["X-1"]}, decode=dict, errors="yield").collect()
        assert yielded[0] == {"n": 1}
        assert isinstance(yielded[1], Forbidden) and yielded[1].grant == "admin"
        assert len(yielded) == 2


async def test_error_is_the_last_item_when_yielded(mock: MockHub) -> None:
    # menger's gate sends the refusal and no `done` after it: "yield" must end there, not wait
    late = mock.expect_call("tickets.delete").reply([data({"n": 1}), FORBIDDEN])
    async with Hub(mock.url, backend="menger") as hub:
        got = await asyncio.wait_for(
            hub.call("tickets.delete", {}, decode=dict, errors="yield").collect(), 2)
        assert got[0] == {"n": 1} and isinstance(got[1], Forbidden) and len(got) == 2
        await late.push(DONE)  # a trailing done for the ended call is dropped
        await asyncio.sleep(0.05)
        assert late.sub not in hub._early and late.sub not in hub._subs


async def test_error_kinds_are_typed(mock: MockHub) -> None:
    mock.expect_call("m.a").reply([error_item("too many calls open", error_kind="too_many_calls", max=256)])
    mock.expect_call("m.b").reply([error_item("sign in", error_kind="unauthenticated")])
    mock.expect_call("m.c").reply([error_item("id: `x` is not a ticket", error_kind="invalid_params")])
    mock.expect_call("m.d").reply([error_item("boom", code="E1", recoverable=True)])
    async with Hub(mock.url, backend="menger") as hub:
        with pytest.raises(TooManyCalls) as tm:
            await hub.call("m.a", {}, decode=dict).collect()
        assert tm.value.max == 256
        with pytest.raises(Unauthenticated):
            await hub.call("m.b", {}, decode=dict).collect()
        with pytest.raises(InvalidParams) as ip:
            await hub.call("m.c", {}, decode=dict).collect()
        assert ip.value.param == "id"
        with pytest.raises(StreamError) as se:
            await hub.call("m.d", {}, decode=dict).collect()
        assert (se.value.code, se.value.recoverable, se.value.message) == ("E1", True, "boom")


async def test_in_band_error_content_is_data(mock: MockHub) -> None:
    mock.expect_call("tickets.create").reply([data({"type": "error", "message": "refused"}), DONE])
    async with Hub(mock.url, backend="menger") as hub:
        got = await hub.call("tickets.create", {}, decode=dict).collect()
    assert got == [{"type": "error", "message": "refused"}]


async def test_rpc_error_invalid_params(mock: MockHub) -> None:
    mock.expect_call("tickets.get").error(-32602, "id: `x` is not a ticket (a ticket is CODE-N)")
    mock.expect_call("tickets.get").error(-32601, "Method not found: tickets.gett")
    async with Hub(mock.url, backend="menger") as hub:
        with pytest.raises(InvalidParams) as ei:
            await hub.call("tickets.get", {"id": "x"}, decode=dict).collect()
        assert isinstance(ei.value, RpcError)
        assert (ei.value.code, ei.value.param) == (-32602, "id")
        with pytest.raises(RpcError) as er:
            await hub.call("tickets.get", {"id": "x"}, decode=dict).collect()
        assert er.value.code == -32601 and not isinstance(er.value, InvalidParams)


async def test_page_and_progress_not_yielded(mock: MockHub) -> None:
    progress = {"type": "progress", "metadata": {}, "message": "half", "percentage": 50.0}
    mock.expect_call("tickets.list").reply([
        progress,
        data({"type": "summary", "id": "X-1"}),
        data({"type": "page", "next": "X-9", "remaining": 3}),
        DONE,
    ])
    mock.expect_call("tickets.list").reply([data({"n": 1}), {"type": "page", "next": None, "remaining": 0, "first": 1}, DONE])
    heard: list[Progress] = []
    async with Hub(mock.url, backend="menger", on_progress=heard.append) as hub:
        stream = hub.call("tickets.list", {}, decode=dict)
        assert await stream.collect() == [{"type": "summary", "id": "X-1"}]
        assert stream.page == Page(next="X-9", remaining=3, first=None)
        assert stream.progress == Progress(message="half", percentage=50.0)
        assert heard == [Progress(message="half", percentage=50.0)]
        second = hub.call("tickets.list", {}, decode=dict)
        assert await second.collect() == [{"n": 1}]
        assert second.page == Page(next=None, remaining=0, first=1)


REQUEST = {"type": "request", "requestId": "r1", "requestData": {"type": "confirm", "message": "ok?"}, "timeoutMs": 5000}


async def test_request_hook_and_default_cancel(mock: MockHub) -> None:
    asked: list[Request] = []

    async def hook(req: Request) -> Any:
        asked.append(req)
        return {"type": "confirmed", "value": True}

    e1 = mock.expect_call("m.ask").reply([REQUEST])
    async with Hub(mock.url, backend="menger", on_request=hook) as hub:
        stream = hub.call("m.ask", {}, decode=dict)
        task = asyncio.create_task(stream.collect())
        frames = await mock.wait_for_frame("menger.respond")
        assert frames[0]["params"] == {"request_id": "r1", "response_data": {"type": "confirmed", "value": True}}
        assert frames[0]["jsonrpc"] == "2.0" and isinstance(frames[0]["id"], int)
        assert asked[0].request_id == "r1" and asked[0].data == {"type": "confirm", "message": "ok?"}
        assert asked[0].timeout_ms == 5000 and asked[0].method == "m.ask"
        await e1.push(data({"n": 1}), DONE)
        assert await task == [{"n": 1}]

    e2 = mock.expect_call("m.ask").reply([REQUEST])
    async with Hub(mock.url, backend="menger") as hub:
        task = asyncio.create_task(hub.call("m.ask", {}, decode=dict).collect())
        frames = await mock.wait_for_frame("menger.respond", count=2)
        assert frames[1]["params"] == {"request_id": "r1", "response_data": {"type": "cancelled"}}
        await e2.push(DONE)
        assert await task == []


async def test_cancel_sends_unsubscribe(mock: MockHub) -> None:
    exp = mock.expect_call("tickets.watch").reply([data({"n": 1}), data({"n": 2})])
    mock.expect_call("tickets.get").reply([data({"n": 9}), DONE])
    async with Hub(mock.url, backend="menger") as hub:
        async for ev in hub.call("tickets.watch", {}, decode=dict):
            assert ev == {"n": 1}
            break
        gc.collect()
        unsub = await mock.wait_for_frame("menger.call_unsub")
        assert unsub[0]["method"] == "menger.call_unsub"
        assert unsub[0]["params"] == [exp.sub] and unsub[0]["jsonrpc"] == "2.0"
        await exp.push(data({"n": 3}), data({"n": 4}))
        assert await hub.call("tickets.get", {}, decode=dict).collect() == [{"n": 9}]
        assert exp.sub not in hub._subs and exp.sub not in hub._early


async def test_aclose_and_async_with_unsubscribe(mock: MockHub) -> None:
    a = mock.expect_call("tickets.watch").reply([data({"n": 1})])
    b = mock.expect_call("tickets.watch").reply([data({"n": 1})])
    async with Hub(mock.url, backend="menger") as hub:
        s = hub.call("tickets.watch", {}, decode=dict)
        assert await s.__anext__() == {"n": 1}
        await s.aclose()
        await s.aclose()  # idempotent
        async with hub.call("tickets.watch", {}, decode=dict) as w:
            assert await w.__anext__() == {"n": 1}
        frames = await mock.wait_for_frame("menger.call_unsub", count=2)
        assert [f["params"] for f in frames] == [[a.sub], [b.sub]]
        with pytest.raises(StopAsyncIteration):
            await s.__anext__()


async def test_preflight_refuses_before_socket(mock: MockHub) -> None:
    pre = {"tickets.delete": Requirement(grant="admin", target="ids", credential=True)}
    async with Hub(mock.url, backend="menger", preflight=pre) as hub:
        with pytest.raises(MissingCredential) as ei:
            await hub.call("tickets.delete", {"ids": ["X-1"]}, decode=dict).collect()
        assert (ei.value.method, ei.value.grant) == ("tickets.delete", "admin")
        assert "admin" in str(ei.value)
    assert mock.recorded == []


async def test_connection_lost_ends_every_stream(mock: MockHub) -> None:
    ea = mock.expect_call("a.watch").reply([data({"a": 1})])
    eb = mock.expect_call("b.watch").reply([data({"b": 1})])
    async with Hub(mock.url, backend="menger") as hub:
        sa = hub.call("a.watch", {}, decode=dict)
        sb = hub.call("b.watch", {}, decode=dict)
        assert await sa.__anext__() == {"a": 1}
        assert await sb.__anext__() == {"b": 1}
        await mock.close_clients(abort=True)
        for s, exp in ((sa, ea), (sb, eb)):
            with pytest.raises(ConnectionLost) as ei:
                await s.__anext__()
            assert (ei.value.method, ei.value.subscription, ei.value.items_seen) == (s.method, exp.sub, 1)
            assert ei.value.closed_by_caller is False
        with pytest.raises(ConnectionLost):
            await hub.call("a.watch", {}, decode=dict).collect()


async def test_close_ends_streams_closed_by_caller(mock: MockHub) -> None:
    mock.expect_call("a.watch").reply([data({"a": 1})])
    hub = Hub(mock.url, backend="menger")
    await hub.connect()
    s = hub.call("a.watch", {}, decode=dict)
    assert await s.__anext__() == {"a": 1}
    await hub.close()
    await hub.close()  # idempotent
    with pytest.raises(ConnectionLost) as ei:
        await s.__anext__()
    assert ei.value.closed_by_caller is True


async def test_backpressure_bounded_queue(mock: MockHub) -> None:
    mock.expect_call("tickets.watch").reply([data({"n": i}) for i in range(100)] + [DONE])
    async with Hub(mock.url, backend="menger", max_queue=8) as hub:
        s = hub.call("tickets.watch", {}, decode=dict)
        assert await s.__anext__() == {"n": 0}
        await asyncio.sleep(0.2)
        assert s.buffered == 8
        rest = await s.collect()
        assert rest == [{"n": i} for i in range(1, 100)]


async def test_refused_upgrade_is_unauthenticated(mock: MockHub) -> None:
    mock.reject_upgrade(401)
    with pytest.raises(Unauthenticated) as ei:
        async with Hub(mock.url, backend="menger"):
            pass
    assert mock.url in str(ei.value)
    assert isinstance(ei.value, PlexusError)


async def test_concurrent_calls_one_socket(mock: MockHub) -> None:
    for i in range(20):
        mock.expect_call("tickets.get").reply([data({"i": i}), DONE])
    async with Hub(mock.url, backend="menger") as hub:
        got = await asyncio.gather(*(hub.call("tickets.get", {"i": i}, decode=dict).one() for i in range(20)))
    assert sorted(g["i"] for g in got) == list(range(20))
    assert len(mock.connections) == 1
    assert sorted(f["id"] for f in mock.calls()) == list(range(1, 21))
