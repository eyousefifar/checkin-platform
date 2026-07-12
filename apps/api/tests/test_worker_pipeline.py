"""Real vision pipeline: thread-safe WS + gallery labels via FaceEngine double."""

from __future__ import annotations

import asyncio
import threading
import time

import numpy as np
import pytest

from app.services.gallery.service import GalleryService
from app.services.vision.embed import pack_embedding
from app.services.vision.engine import MockFaceEngine, set_face_engine
from app.services.vision.worker import VisionWorker, make_synthetic_frame, start_worker
from app.ws.hub import LiveHub


def test_broadcast_nowait_from_worker_thread():
    """Spot-check: messages published from a non-async thread must arrive."""
    hub = LiveHub()
    received: list[dict] = []

    async def run():
        hub.bind_loop()

        class FakeWs:
            def __init__(self):
                self.messages = received

            async def send_json(self, msg):
                self.messages.append(msg)

        ws = FakeWs()
        hub.clients.add(ws)  # type: ignore[arg-type]

        def producer():
            for i in range(5):
                hub.broadcast_nowait(
                    {
                        "type": "detections",
                        "camera_id": "cam_in",
                        "ts": time.time(),
                        "frame_w": 100,
                        "frame_h": 100,
                        "faces": [{"track_id": i, "bbox": [0.1, 0.1, 0.2, 0.2], "label": "T", "score": 0.9, "quality_ok": True, "state": "tracking"}],
                    }
                )
                time.sleep(0.02)

        t = threading.Thread(target=producer)
        t.start()
        # wait for threadsafe coroutines to complete
        deadline = time.time() + 2
        while time.time() < deadline and len(received) < 5:
            await asyncio.sleep(0.05)
        t.join(timeout=2)
        assert len(received) >= 5, f"expected ≥5 messages from thread, got {len(received)}"
        assert all(m["type"] == "detections" for m in received)

    asyncio.run(run())


def test_infer_frame_labels_enrolled_identity():
    """quality → cosine match → label uses gallery, not hard-coded theater."""
    engine = MockFaceEngine(dim=512)
    set_face_engine(engine)

    intensity = 95
    frame = make_synthetic_frame(intensity=intensity, size=280)
    faces = engine.get(frame)
    assert faces, "MockFaceEngine must detect synthetic face"
    emb = faces[0].embedding

    gallery = GalleryService(threshold=0.35, margin=0.02, dim=512)
    gallery.employee_ids = [42]
    gallery.names = ["Enrolled Alice"]
    gallery.matrix = emb.reshape(1, -1).astype(np.float32)
    gallery.version = 7

    import app.services.vision.worker as worker_mod
    import app.services.gallery.service as gallery_mod

    old = gallery_mod._gallery
    gallery_mod._gallery = gallery
    hub = LiveHub()
    worker = VisionWorker(hub)
    worker.trackers["cam_in"] = worker_mod.TrackerState()
    # skip DB reload that would wipe the in-memory gallery fixture
    worker._last_gallery_version = gallery.version
    try:
        out = worker._infer_frame("cam_in", frame)
        # second frame for track continuity still labeled
        out2 = worker._infer_frame("cam_in", make_synthetic_frame(intensity=intensity, size=280, phase=0.2))
    finally:
        gallery_mod._gallery = old
        set_face_engine(None)

    assert out, "expected at least one tracked face"
    assert out[0]["label"] == "Enrolled Alice", out
    assert out[0]["employee_id"] == 42
    assert out[0]["score"] >= 0.35
    assert out[0]["quality_ok"] is True
    assert out2[0]["label"] == "Enrolled Alice"


def test_infer_unknown_when_gallery_empty():
    set_face_engine(MockFaceEngine(dim=512))
    import app.services.gallery.service as gallery_mod
    from app.services.vision.track import TrackerState

    empty = GalleryService(threshold=0.45, margin=0.08)
    old = gallery_mod._gallery
    gallery_mod._gallery = empty
    worker = VisionWorker(LiveHub())
    worker.trackers["cam_in"] = TrackerState()
    try:
        out = worker._infer_frame("cam_in", make_synthetic_frame(intensity=100))
    finally:
        gallery_mod._gallery = old
        set_face_engine(None)
    assert out
    assert out[0]["label"] in ("UNKNOWN", "LOW QUALITY")
    assert out[0]["employee_id"] is None


def test_start_worker_mock_starts_process_threads(tmp_data, monkeypatch):
    """MOCK_VISION must start capture+process threads, not theater-only."""
    monkeypatch.setenv("MOCK_VISION", "true")
    from app.config import get_settings

    get_settings.cache_clear()
    set_face_engine(MockFaceEngine())
    hub = LiveHub()
    # bind a loop in background for safety
    loop = asyncio.new_event_loop()

    def run_loop():
        asyncio.set_event_loop(loop)
        hub.bind_loop(loop)
        loop.run_forever()

    lt = threading.Thread(target=run_loop, daemon=True)
    lt.start()
    time.sleep(0.05)

    w = start_worker(hub, [{"id": "cam_in", "rtsp_url": "", "enabled": True}])
    try:
        names = {t.name for t in w._threads}
        assert any(n.startswith("synth-") or n.startswith("cap-") for n in names)
        assert any(n.startswith("vis-") for n in names)
        # process should mark camera online and produce FPS eventually
        deadline = time.time() + 3
        while time.time() < deadline and not w.online.get("cam_in"):
            time.sleep(0.05)
        assert w.online.get("cam_in") is True
    finally:
        w.stop()
        loop.call_soon_threadsafe(loop.stop)
        set_face_engine(None)
        get_settings.cache_clear()


def test_synthetic_frame_stable_for_engine():
    f = make_synthetic_frame(intensity=80, size=200)
    assert f.shape[0] >= 60 and f.shape[1] >= 60
    eng = MockFaceEngine()
    faces = eng.get(f)
    assert len(faces) == 1
    assert faces[0].embedding.shape == (512,)
