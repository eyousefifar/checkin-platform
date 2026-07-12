from __future__ import annotations

import asyncio

from fastapi import APIRouter, Query, WebSocket, WebSocketDisconnect

from app.auth import verify_token
from app.ws.hub import hub

router = APIRouter(tags=["ws"])


@router.websocket("/ws/live")
async def ws_live(websocket: WebSocket, token: str | None = Query(None)) -> None:
    # Optional token check for LAN demo; allow without if no token provided
    if token:
        try:
            verify_token(token)
        except Exception:  # noqa: BLE001
            await websocket.close(code=4401)
            return
    await hub.connect(websocket)
    try:
        while True:
            try:
                data = await asyncio.wait_for(websocket.receive_json(), timeout=60.0)
            except asyncio.TimeoutError:
                await hub.send(websocket, {"type": "ping", "ts": asyncio.get_event_loop().time()})
                continue
            if isinstance(data, dict) and data.get("type") == "ping":
                await hub.send(websocket, {"type": "pong", "ts": data.get("ts")})
    except WebSocketDisconnect:
        await hub.disconnect(websocket)
    except Exception:  # noqa: BLE001
        await hub.disconnect(websocket)
