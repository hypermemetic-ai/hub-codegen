from __future__ import annotations

from collections.abc import AsyncIterator

import pytest

from .mock_hub import MockHub


@pytest.fixture
async def mock() -> AsyncIterator[MockHub]:
    async with MockHub() as hub:
        yield hub
