"""The typed errors of a Plexus RPC client (PYTHON-8).

Every refusal and failure the runtime surfaces is a ``PlexusError``; a
caller catches the one it can act on (``except Forbidden as e: e.grant``).
Embedded verbatim in every generated package by service-codegen (PYTHON-4).
"""

from __future__ import annotations

import re
from collections.abc import Mapping
from typing import Any, ClassVar

__all__ = [
    "PlexusError",
    "Forbidden",
    "Unauthenticated",
    "RpcError",
    "InvalidParams",
    "TooManyCalls",
    "StreamError",
    "MissingCredential",
    "MissingExtra",
    "CredentialError",
    "ConnectFailed",
    "ConnectionLost",
    "EmptyStream",
    "PlexusDecodeError",
    "error_from_item",
    "error_from_rpc",
]


class PlexusError(Exception):
    """Anything a Plexus service refused, or the runtime could not do.

    ``method`` is the method path the error belongs to (``tickets.delete``),
    when there is one; ``item`` the raw wire object it was read from.
    """

    kind: ClassVar[str] = "error"

    def __init__(self, method: str | None, message: str, *, item: Mapping[str, Any] | None = None) -> None:
        super().__init__(message)
        self.method = method
        self.message = message
        self.item = item

    def __str__(self) -> str:
        return f"{self.method} — {self.message}" if self.method else self.message


class Forbidden(PlexusError):
    """The gate refused the call: ``grant`` on ``scope`` is required (AUTH-7).

    ``roles`` hold the grant; ``held_on`` is what the caller holds
    (``editor@*``) — the TypeScript client's ``ForbiddenError`` fields.
    """

    kind = "forbidden"

    def __init__(self, method: str | None, message: str, *, grant: str, scope: str, roles: list[str],
                 held_on: list[str], item: Mapping[str, Any] | None = None) -> None:
        super().__init__(method, message, item=item)
        self.grant = grant
        self.scope = scope
        self.roles = roles
        self.held_on = held_on

    def __str__(self) -> str:
        return (f"{super().__str__()}\n  error_kind=forbidden grant={self.grant} scope={self.scope} "
                f"held_on=[{', '.join(self.held_on)}]")


class Unauthenticated(PlexusError):
    """The service wants a sign-in: a refused upgrade (401), or ``error_kind: unauthenticated``."""

    kind = "unauthenticated"

    def __init__(self, method: str | None, message: str, *, url: str | None = None,
                 item: Mapping[str, Any] | None = None) -> None:
        super().__init__(method, message, item=item)
        self.url = url


class RpcError(PlexusError):
    """The call itself was answered with a JSON-RPC error (``code``, ``message``)."""

    kind = "rpc"

    def __init__(self, method: str | None, code: int, message: str, *, data: Any = None,
                 item: Mapping[str, Any] | None = None) -> None:
        super().__init__(method, message, item=item)
        self.code = code
        self.data = data

    def __str__(self) -> str:
        return f"{super().__str__()} (code {self.code})"


_PARAM = re.compile(r"^(?:invalid params?:?\s*)?[`'\"]?([A-Za-z_][\w.\-]*)[`'\"]?\s*:", re.IGNORECASE)


class InvalidParams(RpcError):
    """A parameter's type refused its value (``-32602``, ``error_kind: invalid_params``).

    ``param`` is the parameter the message names (``id: `x` is not a ticket``), or None.
    """

    kind = "invalid_params"

    def __init__(self, method: str | None, message: str, *, code: int = -32602, data: Any = None,
                 item: Mapping[str, Any] | None = None) -> None:
        super().__init__(method, code, message, data=data, item=item)
        m = _PARAM.match(message.strip())
        self.param: str | None = m.group(1) if m else None


class TooManyCalls(PlexusError):
    """The connection holds as many open calls as the service allows (``max``); close one."""

    kind = "too_many_calls"

    def __init__(self, method: str | None, message: str, *, max: int | None = None,
                 item: Mapping[str, Any] | None = None) -> None:
        super().__init__(method, message, item=item)
        self.max = max


class StreamError(PlexusError):
    """An ``error`` item with no kind the runtime types: ``message``, ``code``, ``recoverable``."""

    kind = "stream"

    def __init__(self, method: str | None, message: str, *, code: str | None = None, recoverable: bool = False,
                 item: Mapping[str, Any] | None = None) -> None:
        super().__init__(method, message, item=item)
        self.code = code
        self.recoverable = recoverable


class MissingCredential(PlexusError):
    """Refused before anything was sent: ``method`` needs a credential and the service has none."""

    kind = "missing_credential"

    def __init__(self, method: str, grant: str | None) -> None:
        need = f"grant `{grant}`" if grant else "a credential"
        super().__init__(method, f"needs {need}, and this Service has no credentials — pass credentials= "
                                 f"(Token, KeyProof or an async supplier). Nothing was sent.")
        self.grant = grant


class MissingExtra(PlexusError):
    """An optional dependency is not installed: install the package with its ``[extra]``."""

    kind = "missing_extra"

    def __init__(self, extra: str, needed_for: str = "an agent's key proof (Ed25519)") -> None:
        super().__init__(None, f"{needed_for} needs `cryptography`: install the package with its "
                               f"[{extra}] extra (pip install '<package>[{extra}]')")
        self.extra = extra


class CredentialError(PlexusError):
    """A credential could not be read or made: a key file's mode, its length, an agent name."""

    kind = "credential"


class ConnectFailed(PlexusError):
    """The socket could not be opened (unreachable, timed out, not a WebSocket, a refused upgrade but 401/403)."""

    kind = "connect"

    def __init__(self, url: str, message: str, *, status: int | None = None) -> None:
        super().__init__(None, f"{url}: {message}")
        self.url = url
        self.status = status


class ConnectionLost(PlexusError):
    """The socket dropped (or the Service was closed) while the call was open; no reconnect.

    ``subscription`` is the call's id (None if it had none yet); ``items_seen``
    the data items the caller was given; ``closed_by_caller`` when ``Service.close``
    ended it.
    """

    kind = "connection_lost"

    def __init__(self, method: str | None, subscription: int | None = None, items_seen: int = 0, *,
                 closed_by_caller: bool = False, reason: str | None = None) -> None:
        why = "the Service was closed" if closed_by_caller else f"the socket dropped{f' ({reason})' if reason else ''}"
        super().__init__(method, f"{why} with the call open (subscription {subscription}, "
                                 f"{items_seen} item{'s' if items_seen != 1 else ''} seen)")
        self.subscription = subscription
        self.items_seen = items_seen
        self.closed_by_caller = closed_by_caller


class EmptyStream(PlexusError):
    """``MethodStream.one()`` on a call that ended with no data item."""

    kind = "empty"


class PlexusDecodeError(PlexusError):
    """An item's content does not decode to the type the client was generated for."""

    kind = "decode"


def _strs(v: Any) -> list[str]:
    return [str(x) for x in v] if isinstance(v, list) else []


def error_from_item(method: str, item: Mapping[str, Any]) -> PlexusError:
    """The typed error an ``{"type": "error", …}`` stream item stands for."""
    message = str(item.get("message", ""))
    kind = item.get("error_kind")
    if kind == "forbidden":
        return Forbidden(method, message, grant=str(item.get("grant", "")), scope=str(item.get("scope", "")),
                         roles=_strs(item.get("roles")), held_on=_strs(item.get("held_on")), item=item)
    if kind == "unauthenticated":
        return Unauthenticated(method, message, item=item)
    if kind == "invalid_params":
        return InvalidParams(method, message, item=item)
    if kind == "too_many_calls":
        mx = item.get("max")
        return TooManyCalls(method, message, max=mx if isinstance(mx, int) else None, item=item)
    code = item.get("code")
    return StreamError(method, message, code=None if code is None else str(code),
                       recoverable=bool(item.get("recoverable", False)), item=item)


def error_from_rpc(method: str, error: Mapping[str, Any]) -> PlexusError:
    """The typed error a JSON-RPC error response (``{"code", "message", "data"}``) stands for."""
    raw_code = error.get("code", 0)
    code = raw_code if isinstance(raw_code, int) else 0
    message = str(error.get("message", ""))
    data = error.get("data")
    fields: Mapping[str, Any] = data if isinstance(data, Mapping) else {}
    if code == -32602:
        return InvalidParams(method, message, data=data, item=error)
    if code == -32001:
        return Unauthenticated(method, message, item=error)
    if code == -32003:
        return Forbidden(method, message, grant=str(fields.get("grant", "")), scope=str(fields.get("scope", "")),
                         roles=_strs(fields.get("roles")), held_on=_strs(fields.get("held_on")), item=error)
    return RpcError(method, code, message, data=data, item=error)
