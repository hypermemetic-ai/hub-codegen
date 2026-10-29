"""PYTHON-8: credentials — tokens, suppliers and the agent's Ed25519 key proof."""

from __future__ import annotations

import base64
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from unittest import mock as umock

import pytest

from runtime.credentials import KeyProof, Token
from runtime.errors import CredentialError, MissingExtra
from runtime.transport import Service

from .mock_service import MockService

GOLDEN = Path(__file__).parent / "golden" / "key_proof.txt"
SEED = bytes(range(32))
AUD = "ws://127.0.0.1:4486"
AGENT = "python-8-test"
NONCE = bytes([7]) * 32
TS = 1793250000
# menger's checkout beside service-codegen's (or beside its worktree's directory), when there is one
MENGER_CALL = next((p / "menger" / "deploy" / "menger-call.mjs" for p in Path(__file__).resolve().parents
                    if (p / "menger" / "deploy" / "menger-call.mjs").exists()), Path("/nonexistent"))


def _golden() -> str:
    return next(line for line in GOLDEN.read_text().splitlines() if line and not line.startswith("#"))


def _key_file(tmp: Path, seed: bytes = SEED, mode: int = 0o600) -> Path:
    path = tmp / "throwaway.key"
    path.write_text(base64.b64encode(seed).decode() + "\n")
    path.chmod(mode)
    return path


def test_key_proof_matches_menger_call_mjs(tmp_path: Path) -> None:
    proof = KeyProof.from_file(_key_file(tmp_path), agent=AGENT)
    token = proof.token(AUD + "/", nonce=NONCE, ts=TS)  # the trailing / is dropped, as menger-call.mjs does
    assert token == _golden()
    assert proof.principal == "key:03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8"
    if shutil.which("node") and MENGER_CALL.exists():  # the reference itself, when it is beside us
        js = (f"import {{keyProof}} from {json.dumps(str(MENGER_CALL))}; process.stdout.write(keyProof("
              f"Buffer.from([...Array(32).keys()]), '{AUD}', '{AGENT}', Buffer.alloc(32, 7), {TS}))")
        out = subprocess.run(["node", "--input-type=module", "-e", js], capture_output=True, text=True, check=True)
        assert out.stdout == token


def test_key_proof_fresh_nonce_each_header(tmp_path: Path) -> None:
    proof = KeyProof.from_file(_key_file(tmp_path), agent=AGENT)
    a, b = proof.token(AUD), proof.token(AUD)
    assert a != b
    body = json.loads(base64.urlsafe_b64decode(a.split(".", 1)[1] + "=="))
    assert list(body) == ["v", "key", "aud", "agent", "nonce", "ts", "sig"]
    assert body["aud"] == AUD and body["agent"] == AGENT and len(base64.urlsafe_b64decode(body["nonce"] + "=")) == 32


def test_key_file_mode_refused(tmp_path: Path) -> None:
    path = _key_file(tmp_path, mode=0o644)
    with pytest.raises(CredentialError) as ei:
        KeyProof.from_file(path, agent=AGENT)
    assert "0644" in str(ei.value) and str(path) in str(ei.value)
    path.chmod(0o600)
    assert KeyProof.from_file(path, agent=AGENT).agent == AGENT


def test_key_refusals_named(tmp_path: Path) -> None:
    with pytest.raises(CredentialError, match="agent"):
        KeyProof.from_file(_key_file(tmp_path), agent="no spaces")
    with pytest.raises(CredentialError, match="agent"):
        KeyProof(SEED, agent="x" * 65)
    with pytest.raises(CredentialError, match="32-byte"):
        KeyProof.from_file(_key_file(tmp_path, seed=b"short"), agent=AGENT)
    assert "AAEC" not in repr(KeyProof(SEED, agent=AGENT))  # the seed is never shown


def test_missing_extra_is_named() -> None:
    hidden = {k: None for k in list(sys.modules) if k == "cryptography" or k.startswith("cryptography.")}
    hidden["cryptography"] = None
    with umock.patch.dict(sys.modules, hidden):
        with pytest.raises(MissingExtra) as ei:
            KeyProof(SEED, agent=AGENT)
    assert ei.value.extra == "agent"
    assert "[agent]" in str(ei.value)
    # and a process with no cryptography at all imports the runtime, and says so only when a key is made
    code = ("import sys; sys.modules['cryptography'] = None\n"
            "import runtime.transport, runtime.credentials\n"
            "from runtime.errors import MissingExtra\n"
            "try:\n    runtime.credentials.KeyProof(bytes(32), agent='a')\n"
            "except MissingExtra as e:\n    print('missing', e.extra)\n")
    env = {**os.environ, "PYTHONPATH": str(Path(__file__).resolve().parents[2])}
    out = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, check=True, env=env)
    assert out.stdout.strip() == "missing agent"


def test_importing_transport_does_not_import_cryptography() -> None:
    code = "import sys, runtime.transport, runtime.credentials; print('cryptography' in sys.modules)"
    env = {**os.environ, "PYTHONPATH": str(Path(__file__).resolve().parents[2])}
    out = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, check=True, env=env)
    assert out.stdout.strip() == "False"


async def test_token_file_reread_each_connect(mock: MockService, tmp_path: Path) -> None:
    path = tmp_path / "token"
    path.write_text("first\n")
    path.chmod(0o600)
    service = Service(mock.url, backend="menger", credentials=Token.from_file(path))
    async with service:
        pass
    path.write_text("second\n")
    async with service:
        pass
    assert [h["Authorization"] for h in mock.upgrade_headers] == ["Bearer first", "Bearer second"]


async def test_token_and_supplier_sent_as_bearer(mock: MockService) -> None:
    async def supply() -> str:
        return "from-supplier"

    async with Service(mock.url, backend="menger", credentials=Token("plain")):
        pass
    async with Service(mock.url, backend="menger", credentials=supply):
        pass
    async with Service(mock.url, backend="menger"):
        pass
    got = [h.get("Authorization") for h in mock.upgrade_headers]
    assert got == ["Bearer plain", "Bearer from-supplier", None]
    assert "plain" not in repr(Token("plain"))


async def test_key_proof_sent_at_upgrade(mock: MockService, tmp_path: Path) -> None:
    proof = KeyProof.from_file(_key_file(tmp_path), agent=AGENT)
    async with Service(mock.url + "/", backend="menger", credentials=proof):
        pass
    value = mock.upgrade_headers[0]["Authorization"]
    assert value.startswith("Bearer plexus-key.")
    body = json.loads(base64.urlsafe_b64decode(value.split(".", 1)[1] + "=="))
    assert body["aud"] == mock.url and body["agent"] == AGENT
