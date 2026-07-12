"""DB engine and session."""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path

from sqlalchemy import create_engine, select
from sqlalchemy.orm import Session, sessionmaker

from app.config import get_settings
from app.db.models import AppMeta, Base, Camera

engine = None
SessionLocal = None


def _make_engine(url: str | None = None):
    settings = get_settings()
    db_url = url or settings.resolved_database_url()
    connect_args = {}
    if db_url.startswith("sqlite"):
        connect_args["check_same_thread"] = False
        path = db_url.replace("sqlite:///", "", 1)
        Path(path).parent.mkdir(parents=True, exist_ok=True)
    return create_engine(db_url, connect_args=connect_args, future=True)


def init_engine(url: str | None = None):
    global engine, SessionLocal
    engine = _make_engine(url)
    SessionLocal = sessionmaker(bind=engine, autoflush=False, autocommit=False, future=True)
    return engine


def get_db() -> Generator[Session, None, None]:
    if SessionLocal is None:
        init_engine()
    assert SessionLocal is not None
    db = SessionLocal()
    try:
        yield db
    finally:
        db.close()


def seed_cameras(db: Session) -> None:
    settings = get_settings()
    existing = db.scalars(select(Camera)).all()
    if existing:
        return
    cams = [
        Camera(
            id="cam_in",
            name="Entrance",
            rtsp_url=settings.cam_in_rtsp,
            webrtc_path=settings.cam_in_webrtc_path,
            direction=settings.cam_in_direction,
            enabled=True,
            sort_order=0,
        ),
    ]
    if settings.cam_out_rtsp:
        cams.append(
            Camera(
                id="cam_out",
                name="Exit",
                rtsp_url=settings.cam_out_rtsp,
                webrtc_path=settings.cam_out_webrtc_path,
                direction=settings.cam_out_direction,
                enabled=True,
                sort_order=1,
            )
        )
    db.add_all(cams)
    if db.get(AppMeta, "gallery_version") is None:
        db.add(AppMeta(key="gallery_version", value="0"))
    db.commit()


def init_db(url: str | None = None) -> None:
    settings = get_settings()
    settings.resolved_data_dir.mkdir(parents=True, exist_ok=True)
    settings.enroll_dir.mkdir(parents=True, exist_ok=True)
    eng = init_engine(url)
    Base.metadata.create_all(bind=eng)
    assert SessionLocal is not None
    db = SessionLocal()
    try:
        seed_cameras(db)
    finally:
        db.close()
