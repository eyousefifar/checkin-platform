from __future__ import annotations

from fastapi import APIRouter, Depends
from sqlalchemy import select
from sqlalchemy.orm import Session

from app.auth import require_auth
from app.db.models import Camera
from app.db.session import get_db
from app.services.vision.worker import get_worker

router = APIRouter(prefix="/cameras", tags=["cameras"])


@router.get("")
def list_cameras(
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> list[dict]:
    worker = get_worker()
    rows = db.scalars(select(Camera).order_by(Camera.sort_order)).all()
    out = []
    for c in rows:
        online = False
        if worker and c.id in worker.online:
            online = worker.online[c.id]
        out.append(
            {
                "id": c.id,
                "name": c.name,
                "direction": c.direction,
                "enabled": c.enabled,
                "webrtc_path": c.webrtc_path,
                "rtsp_url": c.rtsp_url,
                "online": online,
            }
        )
    return out
