"""FastAPI entrypoint — PKSP Check-In API."""

from __future__ import annotations

import logging
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from sqlalchemy import select

from app.config import get_settings
from app.db.models import Camera
from app.db import session as db_session
from app.routers import attendance, auth, cameras, employees, health, ws
from app.services.gallery.service import get_gallery
from app.services.vision.engine import get_face_engine
from app.services.vision.worker import start_worker
from app.ws.hub import hub

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("pksp")


@asynccontextmanager
async def lifespan(app: FastAPI):
    import asyncio

    settings = get_settings()
    settings.resolved_data_dir.mkdir(parents=True, exist_ok=True)
    settings.enroll_dir.mkdir(parents=True, exist_ok=True)
    db_session.init_db()
    # Bind WS hub to this event loop so worker threads can publish safely
    hub.bind_loop(asyncio.get_running_loop())

    engine = get_face_engine()
    provider = getattr(engine, "execution_provider", None) or ("mock" if settings.mock_vision else "unknown")
    logger.info(
        "Face engine ready=%s model=%s provider=%s mock_vision=%s",
        getattr(engine, "ready", False),
        getattr(engine, "model_name", "?"),
        provider,
        settings.mock_vision,
    )
    if db_session.SessionLocal is None:
        raise RuntimeError("DB session factory not initialized")
    db = db_session.SessionLocal()
    try:
        gallery = get_gallery()
        gallery.threshold = settings.match_threshold
        gallery.margin = settings.match_margin
        gallery.load(db)
        hub.gallery_version = gallery.version
        cams = db.scalars(select(Camera)).all()
        cam_list = [
            {
                "id": c.id,
                "rtsp_url": c.rtsp_url,
                "enabled": c.enabled,
                "webrtc_path": c.webrtc_path,
            }
            for c in cams
        ]
    finally:
        db.close()

    if settings.vision_enabled:
        start_worker(hub, cam_list)
    yield
    hub.stop_mock()
    from app.services.vision.worker import get_worker

    w = get_worker()
    if w:
        w.stop()


def create_app() -> FastAPI:
    settings = get_settings()
    app = FastAPI(title="PKSP Check-In API", version="0.1.0", lifespan=lifespan)
    app.add_middleware(
        CORSMiddleware,
        allow_origins=settings.cors_origin_list or ["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )
    app.include_router(health.router, prefix="/api")
    app.include_router(auth.router, prefix="/api")
    app.include_router(employees.router, prefix="/api")
    app.include_router(attendance.router, prefix="/api")
    app.include_router(cameras.router, prefix="/api")
    app.include_router(ws.router, prefix="/api")
    return app


app = create_app()
