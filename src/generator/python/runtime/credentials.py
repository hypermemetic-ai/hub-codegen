"""What a Hub signs in with (PYTHON-8): a token, an async supplier, or an
agent's Ed25519 key proof — always sent as ``Authorization: Bearer …`` at the
WebSocket upgrade, and always read from a file, never argv or the environment.

``KeyProof`` is IDENTITY-40's proof, byte for byte what menger's
``deploy/menger-call.mjs`` ``keyProof()`` and synapse sign:

    Bearer plexus-key.<base64url JSON {v:1, key, aud, agent, nonce, ts, sig}>
    sig = Ed25519(seed, "plexus-key-login/v1\\n<aud>\\n<agent>\\n<nonce>\\n<ts>")

It needs ``cryptography`` (the package's ``[agent]`` extra), imported only
when a KeyProof is made.
"""

from __future__ import annotations

import base64
import json
import os
import re
import secrets
import stat
import time
from collections.abc import Awaitable, Callable
from pathlib import Path
from typing import Any, Union

from .errors import CredentialError, MissingExtra

__all__ = ["Credentials", "Token", "KeyProof", "authorization"]

_AGENT = re.compile(r"^[A-Za-z0-9._-]{1,64}$")


def _b64url(b: bytes) -> str:
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode("ascii")


def _private_file(path: Path, what: str) -> str:
    """The text of ``path``, refused when group or others may read it."""
    try:
        st = path.stat()
    except OSError as e:
        raise CredentialError(None, f"{what} {path}: {e.strerror or e}") from None
    if st.st_mode & 0o077:
        raise CredentialError(None, f"{what} {path} is readable by others (mode "
                                    f"{stat.S_IMODE(st.st_mode):04o}; make it 0600)")
    return path.read_text(encoding="utf-8")


class Token:
    """A bearer token: given, or read from a file (0600) at every connect."""

    __slots__ = ("_text", "_path")

    def __init__(self, text: str) -> None:
        self._text: str | None = text.strip()
        self._path: Path | None = None

    @classmethod
    def from_file(cls, path: str | os.PathLike[str]) -> Token:
        tok = cls("")
        tok._text = None
        tok._path = Path(path).expanduser()
        return tok

    def token(self) -> str:
        if self._path is not None:
            return _private_file(self._path, "token file").strip()
        assert self._text is not None
        return self._text

    def __repr__(self) -> str:
        return f"Token.from_file({str(self._path)!r})" if self._path else "Token(<hidden>)"


class KeyProof:
    """An agent's Ed25519 key, proving itself to a hub at each connect (IDENTITY-40).

    ``KeyProof.from_file("~/.plexus/keys/builder.key", agent="builder")`` — the
    file holds the 32-byte seed in base64 (``synapse _key new NAME --store
    file``) and must be 0600. A fresh 32-byte nonce and the current time go
    into every proof; the audience is the URL dialed, without a trailing ``/``.
    """

    __slots__ = ("agent", "_key", "principal")

    def __init__(self, seed: bytes, *, agent: str) -> None:
        if not _AGENT.match(agent):
            raise CredentialError(None, f"agent {agent!r}: 1–64 of A-Z a-z 0-9 . _ -")
        if len(seed) != 32:
            raise CredentialError(None, f"a key is a 32-byte Ed25519 seed in base64, not {len(seed)} bytes")
        try:
            from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
            from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat
        except ImportError:
            raise MissingExtra("agent") from None
        self.agent = agent
        self._key = Ed25519PrivateKey.from_private_bytes(seed)
        public = self._key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
        self.principal = "key:" + public.hex()

    @classmethod
    def from_seed_text(cls, text: str, *, agent: str) -> KeyProof:
        t = text.strip().replace("-", "+").replace("_", "/")
        try:
            seed = base64.b64decode(t + "=" * (-len(t) % 4))
        except ValueError:
            raise CredentialError(None, "a key is a 32-byte Ed25519 seed in base64") from None
        return cls(seed, agent=agent)

    @classmethod
    def from_file(cls, path: str | os.PathLike[str], *, agent: str) -> KeyProof:
        return cls.from_seed_text(_private_file(Path(path).expanduser(), "key file"), agent=agent)

    def token(self, aud: str, *, nonce: bytes | None = None, ts: int | None = None) -> str:
        """``plexus-key.<…>`` for audience ``aud``; ``nonce`` / ``ts`` fixed only by tests."""
        aud = aud.rstrip("/")
        nonce_s = _b64url(nonce if nonce is not None else secrets.token_bytes(32))
        when = int(time.time()) if ts is None else ts
        msg = "\n".join(["plexus-key-login/v1", aud, self.agent, nonce_s, str(when)])
        sig = self._key.sign(msg.encode("utf-8"))
        body: dict[str, Any] = {"v": 1, "key": self.principal[4:], "aud": aud, "agent": self.agent,
                                "nonce": nonce_s, "ts": when, "sig": _b64url(sig)}
        return "plexus-key." + _b64url(json.dumps(body, separators=(",", ":"), ensure_ascii=False).encode("utf-8"))

    def __repr__(self) -> str:
        return f"KeyProof({self.principal}, agent={self.agent!r})"


Credentials = Union[Token, KeyProof, Callable[[], Awaitable[str]]]
"""What ``Hub(credentials=…)`` takes."""


async def authorization(credentials: Credentials, url: str) -> str:
    """The ``Authorization`` header's value for one connect to ``url``."""
    if isinstance(credentials, KeyProof):
        return "Bearer " + credentials.token(url)
    token = credentials.token() if isinstance(credentials, Token) else await credentials()
    return token if token.startswith("Bearer ") else "Bearer " + token
