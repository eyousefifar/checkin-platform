"""WebSocket broadcast hub — thread-safe for vision worker threads."""

from __future__ import annotations

import asyncio
import logging
import time
from typing import Any

from fastapi import WebSocket

logger = logging.getLogger(__name__)


class LiveHub:
    def __init__(self) -> None:
        self.clients: set[WebSocket] = set()
        self.gallery_version: int = 0
        self._async_lock: asyncio.Lock | None = None
        self._loop: asyncio.AbstractEventLoop | None = None
        self._clients_guard = asyncio.Lock  # type placeholder; real lock set in bind_loop
        self.mock_task: asyncio.Task | None = None
        self.metrics: dict[str, Any] = {
            "cameras_online": 0,
            "present_count": 0,
            "events_today": 0,
            "vision_fps": {},
        }
        self.camera_online: dict[str, bool] = {}
        self._client_set_lock = None  # set on bind

    def bind_loop(self, loop: asyncio.AbstractEventLoop | None = None) -> None:
        """Call from app lifespan on the API event loop so worker threads can publish."""
        self._loop = loop or asyncio.get_running_loop()
        if self._async_lock is None:
            self._async_lock = asyncio.Lock()

    async def connect(self, ws: WebSocket) -> None:
        await ws.accept()
        if self._async_lock is None:
            self._async_lock = asyncio.Lock()
        if self._loop is None:
            self.bind_loop()
        async with self._async_lock:
            self.clients.add(ws)
        await self.send(
            ws,
            {
                "type": "hello",
                "server_ts": time.time(),
                "gallery_version": self.gallery_version,
            },
        )

    async def disconnect(self, ws: WebSocket) -> None:
        if self._async_lock is None:
            self.clients.discard(ws)
            return
        async with self._async_lock:
            self.clients.discard(ws)

    async def send(self, ws: WebSocket, message: dict) -> None:
        await ws.send_json(message)

    async def broadcast(self, message: dict) -> None:
        if self._async_lock is None:
            self._async_lock = asyncio.Lock()
        dead: list[WebSocket] = []
        async with self._async_lock:
            clients = list(self.clients)
        for ws in clients:
            try:
                await ws.send_json(message)
            except Exception:  # noqa: BLE001
                dead.append(ws)
        for ws in dead:
            await self.disconnect(ws)

    def broadcast_nowait(self, message: dict) -> None:
        """Safe from worker threads and from the event loop."""
        try:
            running = asyncio.get_running_loop()
        except RuntimeError:
            running = None

        if running is not None:
            running.create_task(self.broadcast(message))
            return

        loop = self._loop
        if loop is None or not loop.is_running():
            logger.debug("broadcast_nowait dropped: no event loop bound")
            return
        try:
            asyncio.run_coroutine_threadsafe(self.broadcast(message), loop)
        except Exception as exc:  # noqa: BLE001
            logger.warning("broadcast_nowait failed: %s", exc)

    def start_mock(self) -> None:
        """Deprecated theater path — kept no-op for compatibility; real worker is used."""
        return

    def stop_mock(self) -> None:
        if self.mock_task and not self.mock_task.done():
            self.mock_task.cancel()
            self.mock_task = None


hub = LiveHub()
