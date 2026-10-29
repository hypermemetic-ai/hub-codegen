"""PYTHON-8: the helpers every generated decoder uses (PYTHON-4)."""

from __future__ import annotations

import pytest

from runtime._decode import DecodeError, lit, opt, req
from runtime.errors import PlexusDecodeError, PlexusError

STATUS = frozenset({"Draft", "In Review"})


def test_req_names_type_and_field() -> None:
    assert req({"id": "X-1"}, "id", "Ticket") == "X-1"
    with pytest.raises(DecodeError) as ei:
        req({}, "id", "Ticket")
    assert (ei.value.type_name, ei.value.field) == ("Ticket", "id")
    assert "Ticket.id" in str(ei.value)
    assert isinstance(ei.value, PlexusDecodeError) and isinstance(ei.value, PlexusError)
    with pytest.raises(DecodeError, match="object"):
        req(["not", "a", "dict"], "id", "Ticket")


def test_opt_is_none_when_absent_or_null() -> None:
    assert opt({"a": 1}, "a") == 1
    assert opt({"a": None}, "a") is None
    assert opt({}, "a") is None


def test_lit_checks_membership() -> None:
    assert lit("In Review", STATUS, "Status") == "In Review"
    with pytest.raises(DecodeError, match="'Shipped'"):
        lit("Shipped", STATUS, "Status")
    with pytest.raises(DecodeError):
        lit(3, STATUS, "Status")
    with pytest.raises(DecodeError) as ei:
        lit("x", STATUS, "Ticket", field="status")
    assert ei.value.field == "status"


def test_decode_error_as_the_generator_raises_it() -> None:
    e = DecodeError("ChatsEvent", "type", "unknown variant 'zap'")
    assert str(e) == "ChatsEvent.type: unknown variant 'zap'"
