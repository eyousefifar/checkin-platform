from __future__ import annotations

from fastapi import APIRouter, Depends
from sqlalchemy import select
from sqlalchemy.orm import Session

from app.db.models import Camera
from app.db.session import get_db
from app.services.gallery.service import get_gallery
from app.services.vision.engine import get_face_engine

router = APIRouter(tags=["health"])


@router.get("/health")
def health(db: Session = Depends(get_db)) -> dict:
    gallery = get_gallery()
    engine = get_face_engine()
    cams = db.scalars(select(Camera).where(Camera.enabled.is_(True))).all()
    return {
        "status": "ok",
        "vision_ready": bool(getattr(engine, "ready", False)),
        "gallery_size": gallery.size(),
        "cameras": [
            {
                "id": c.id,
                "name": c.name,
                "direction": c.direction,
                "enabled": c.enabled,
                "webrtc_path": c.webrtc_path,
            }
            for c in cams
        ],
    }
