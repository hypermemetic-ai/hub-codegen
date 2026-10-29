"""The helpers every generated decoder is written with (PYTHON-8, for PYTHON-4).

A generated decoder is one expression per field:

    Ticket(id=req(d, "id", "Ticket"), parent=opt(d, "parent"),
           status=lit(req(d, "status", "Ticket"), _STATUS, "Status"))

and a bad item raises ``DecodeError`` naming the type and the field.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

from .errors import PlexusDecodeError

__all__ = ["DecodeError", "req", "opt", "lit"]


class DecodeError(PlexusDecodeError):
    """``<type_name>.<field>: <problem>`` — an item does not decode to its type."""

    def __init__(self, type_name: str, field: str | None, problem: str) -> None:
        where = f"{type_name}.{field}" if field else type_name
        super().__init__(None, f"{where}: {problem}")
        self.type_name = type_name
        self.field = field
        self.problem = problem


def req(d: Any, key: str, type_name: str) -> Any:
    """``d[key]``, or ``DecodeError`` naming ``type_name`` and ``key`` when it is absent."""
    if not isinstance(d, Mapping):
        raise DecodeError(type_name, key, f"expected an object, got {type(d).__name__}")
    try:
        return d[key]
    except KeyError:
        raise DecodeError(type_name, key, "missing") from None


def opt(d: Any, key: str) -> Any | None:
    """``d[key]``, or None when it is absent or null."""
    if not isinstance(d, Mapping):
        return None
    return d.get(key)


def lit(value: Any, allowed: frozenset[str], type_name: str, field: str | None = None) -> str:
    """``value`` when it is one of ``allowed`` (a string enum's ``Literal``), else ``DecodeError``."""
    if isinstance(value, str) and value in allowed:
        return value
    raise DecodeError(type_name, field, f"{value!r} is not one of {sorted(allowed)}")
