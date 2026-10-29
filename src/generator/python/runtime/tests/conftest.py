from __future__ import annotations

from collections.abc import AsyncIterator

import pytest

from .mock_service import MockService


@pytest.fixture
async def mock() -> AsyncIterator[MockService]:
    async with MockService() as service:
        yield service
