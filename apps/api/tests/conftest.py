"""Shared fixtures — isolated SQLite + mock face engine."""

from __future__ import annotations

import os
from pathlib import Path

import numpy as np
import pytest
from fastapi.testclient import TestClient

# Force test env before app imports settings cache
os.environ["ADMIN_PASSWORD"] = "test-admin"
os.environ["JWT_SECRET"] = "test-jwt-secret"
os.environ["MOCK_VISION"] = "true"
os.environ["VISION_ENABLED"] = "true"
os.environ["MIN_ENROLL_IMAGES"] = "1"
os.environ["COOLDOWN_SECONDS"] = "90"
os.environ["MIN_DWELL_SECONDS"] = "30"
os.environ["MATCH_THRESHOLD"] = "0.45"
os.environ["MATCH_MARGIN"] = "0.08"


@pytest.fixture()
def tmp_data(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    db_path = tmp_path / "test.db"
    data_dir = tmp_path / "data"
    data_dir.mkdir()
    monkeypatch.setenv("DATABASE_URL", f"sqlite:///{db_path}")
    monkeypatch.setenv("DATA_DIR", str(data_dir))
    # clear settings cache
    from app.config import get_settings

    get_settings.cache_clear()
    yield tmp_path
    get_settings.cache_clear()


@pytest.fixture()
def client(tmp_data, monkeypatch: pytest.MonkeyPatch):
    from app.config import get_settings
    from app.db.session import init_db
    from app.services.gallery.service import reset_gallery
    from app.services.vision.engine import MockFaceEngine, set_face_engine

    get_settings.cache_clear()
    reset_gallery()
    set_face_engine(MockFaceEngine(dim=512))
    init_db()

    # disable mock broadcaster noise in tests via short-lived app
    from app.main import create_app
    from app.ws.hub import hub

    app = create_app()
    with TestClient(app) as c:
        # lifespan starts mock theater when MOCK_VISION=true; keep it for WS tests
        yield c
    hub.stop_mock()
    set_face_engine(None)
    reset_gallery()
    get_settings.cache_clear()


@pytest.fixture()
def auth_header(client: TestClient) -> dict[str, str]:
    r = client.post("/api/auth/login", json={"password": "test-admin"})
    assert r.status_code == 200, r.text
    token = r.json()["access_token"]
    return {"Authorization": f"Bearer {token}"}


def make_face_image(seed: int = 1, size: int = 200) -> bytes:
    """Synthetic BGR-ish JPEG with stable mean for MockFaceEngine buckets."""
    import cv2

    rng = np.random.default_rng(seed)
    # base intensity drives mock embedding bucket
    base = 40 + (seed * 17) % 180
    img = np.full((size, size, 3), base, dtype=np.uint8)
    noise = rng.integers(0, 20, size=(size, size, 3), dtype=np.uint8)
    img = np.clip(img.astype(np.int16) + noise, 0, 255).astype(np.uint8)
    # draw a face-like oval so OpenCV encode works
    cv2.ellipse(img, (size // 2, size // 2), (size // 3, size // 2 - 10), 0, 0, 360, (base + 30, base + 20, base + 10), -1)
    ok, buf = cv2.imencode(".jpg", img)
    assert ok
    return buf.tobytes()


def make_blank_image() -> bytes:
    import cv2

    img = np.zeros((100, 100, 3), dtype=np.uint8)
    ok, buf = cv2.imencode(".jpg", img)
    assert ok
    return buf.tobytes()
