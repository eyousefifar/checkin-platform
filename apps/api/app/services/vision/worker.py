"""Live vision worker: RTSP/latest-frame + throttle + pipeline + WS.

When MOCK_VISION=true, synthetic frames feed the same quality→match→IoU→vote→
attendance path using MockFaceEngine (honest double), not a hard-coded theater.
"""

from __future__ import annotations

import logging
import threading
import time
from typing import Any

import numpy as np
from sqlalchemy.orm import Session

from app.config import get_settings
from app.db import session as db_session
from app.services.attendance.service import commit_identity
from app.services.gallery.service import get_gallery
from app.services.vision.engine import get_face_engine
from app.services.vision.quality import quality_gate
from app.services.vision.track import TrackerState, assign_tracks
from app.services.vision.vote import evaluate_vote

logger = logging.getLogger(__name__)


class LatestFrameBuffer:
    def __init__(self) -> None:
        self._frame: np.ndarray | None = None
        self._ts: float = 0.0
        self._lock = threading.Lock()

    def set(self, frame: np.ndarray) -> None:
        with self._lock:
            self._frame = frame
            self._ts = time.time()

    def get(self) -> tuple[np.ndarray | None, float]:
        with self._lock:
            return self._frame, self._ts


def make_synthetic_frame(
    *,
    intensity: int = 80,
    size: int = 320,
    phase: float = 0.0,
) -> np.ndarray:
    """BGR frame with a face-like region; intensity drives MockFaceEngine bucket."""
    import cv2

    intensity = int(np.clip(intensity, 20, 220))
    img = np.full((size, size, 3), max(intensity - 15, 10), dtype=np.uint8)
    # moving face center for tracker exercise
    cx = int(size * (0.5 + 0.08 * np.sin(phase)))
    cy = int(size * (0.45 + 0.05 * np.cos(phase * 0.7)))
    axes = (size // 4, size // 3)
    color = (intensity, min(intensity + 20, 255), min(intensity + 10, 255))
    cv2.ellipse(img, (cx, cy), axes, 0, 0, 360, color, -1)
    # eyes
    cv2.circle(img, (cx - axes[0] // 3, cy - axes[1] // 5), 6, (20, 20, 20), -1)
    cv2.circle(img, (cx + axes[0] // 3, cy - axes[1] // 5), 6, (20, 20, 20), -1)
    return img


class VisionWorker:
    def __init__(self, hub: Any):
        self.hub = hub
        self._stop = threading.Event()
        self._threads: list[threading.Thread] = []
        self._lock = threading.Lock()  # inference lock
        self.trackers: dict[str, TrackerState] = {}
        self.online: dict[str, bool] = {}
        self.fps: dict[str, float] = {}
        self._last_gallery_version = -1

    def stop(self) -> None:
        self._stop.set()
        for t in self._threads:
            t.join(timeout=2)
        self._threads.clear()

    def start_background(self, cameras: list[dict], *, synthetic: bool = False) -> None:
        """cameras: {id, rtsp_url, enabled}"""
        settings = get_settings()
        self._stop.clear()
        started = False
        for cam in cameras:
            if not cam.get("enabled", True):
                continue
            use_synth = synthetic or not cam.get("rtsp_url")
            if not use_synth and not cam.get("rtsp_url"):
                continue
            buf = LatestFrameBuffer()
            cam_id = cam["id"]
            if use_synth:
                cap_t = threading.Thread(
                    target=self._synthetic_capture_loop,
                    args=(cam_id, buf),
                    daemon=True,
                    name=f"synth-{cam_id}",
                )
            else:
                cap_t = threading.Thread(
                    target=self._capture_loop,
                    args=(cam_id, cam["rtsp_url"], buf),
                    daemon=True,
                    name=f"cap-{cam_id}",
                )
            proc_t = threading.Thread(
                target=self._process_loop,
                args=(cam_id, buf, settings.vision_target_fps),
                daemon=True,
                name=f"vis-{cam_id}",
            )
            self._threads.extend([cap_t, proc_t])
            cap_t.start()
            proc_t.start()
            started = True
        if not started:
            # always at least one synthetic cam for demo
            buf = LatestFrameBuffer()
            cam_id = "cam_in"
            cap_t = threading.Thread(
                target=self._synthetic_capture_loop,
                args=(cam_id, buf),
                daemon=True,
                name=f"synth-{cam_id}",
            )
            proc_t = threading.Thread(
                target=self._process_loop,
                args=(cam_id, buf, settings.vision_target_fps),
                daemon=True,
                name=f"vis-{cam_id}",
            )
            self._threads.extend([cap_t, proc_t])
            cap_t.start()
            proc_t.start()

    def _load_enroll_frames(self) -> list[np.ndarray]:
        """Optional demo frames from on-disk enroll photos (same path as live RTSP content)."""
        import cv2

        settings = get_settings()
        root = settings.resolved_data_dir / "enroll"
        frames: list[np.ndarray] = []
        if not root.exists():
            return frames
        for path in sorted(root.rglob("*")):
            if path.suffix.lower() not in {".jpg", ".jpeg", ".png", ".webp", ".bmp"}:
                continue
            img = cv2.imread(str(path))
            if img is not None and img.size:
                frames.append(img)
            if len(frames) >= 24:
                break
        return frames

    def _synthetic_capture_loop(self, camera_id: str, buf: LatestFrameBuffer) -> None:
        """Feed latest-frame buffer without RTSP — still runs real process pipeline.

        Prefer cycling enrolled images from disk (honest match path). Fall back to
        geometric synthetic faces for empty galleries.
        """
        t0 = time.time()
        enroll_frames = self._load_enroll_frames()
        last_reload = 0.0
        idx = 0
        while not self._stop.is_set():
            now = time.time()
            # reload enroll photos periodically so mid-session enroll appears
            if now - last_reload > 2.0:
                enroll_frames = self._load_enroll_frames()
                last_reload = now
            phase = now - t0
            if enroll_frames:
                frame = enroll_frames[idx % len(enroll_frames)].copy()
                idx += 1
            else:
                intensity = 40 + (int(phase) * 17) % 180
                frame = make_synthetic_frame(intensity=intensity, phase=phase)
            buf.set(frame)
            self.online[camera_id] = True
            time.sleep(0.08)

    def _capture_loop(self, camera_id: str, rtsp_url: str, buf: LatestFrameBuffer) -> None:
        import cv2

        while not self._stop.is_set():
            cap = cv2.VideoCapture(rtsp_url, cv2.CAP_FFMPEG)
            if not cap.isOpened():
                self.online[camera_id] = False
                self.hub.broadcast_nowait(
                    {
                        "type": "camera_status",
                        "camera_id": camera_id,
                        "online": False,
                        "last_frame_age_ms": None,
                    }
                )
                time.sleep(2)
                continue
            self.online[camera_id] = True
            while not self._stop.is_set():
                ok, frame = cap.read()
                if not ok or frame is None:
                    self.online[camera_id] = False
                    break
                buf.set(frame)
            cap.release()
            time.sleep(1)

    def _process_loop(self, camera_id: str, buf: LatestFrameBuffer, target_fps: float) -> None:
        interval = 1.0 / max(target_fps, 0.5)
        self.trackers.setdefault(camera_id, TrackerState())
        last = 0.0
        frames = 0
        fps_t0 = time.time()
        metrics_t0 = time.time()

        while not self._stop.is_set():
            now = time.time()
            if now - last < interval:
                time.sleep(0.005)
                continue
            last = now
            frame, fts = buf.get()
            if frame is None:
                continue
            age_ms = int((time.time() - fts) * 1000)

            try:
                faces_out = self._infer_frame(camera_id, frame)
            except Exception as exc:  # noqa: BLE001
                logger.exception("vision infer failed: %s", exc)
                continue

            frames += 1
            if time.time() - fps_t0 >= 2:
                self.fps[camera_id] = frames / (time.time() - fps_t0)
                frames = 0
                fps_t0 = time.time()

            h, w = frame.shape[:2]
            self.hub.broadcast_nowait(
                {
                    "type": "detections",
                    "camera_id": camera_id,
                    "ts": time.time(),
                    "frame_w": w,
                    "frame_h": h,
                    "faces": faces_out,
                }
            )
            self.hub.broadcast_nowait(
                {
                    "type": "camera_status",
                    "camera_id": camera_id,
                    "online": True,
                    "last_frame_age_ms": age_ms,
                }
            )
            if time.time() - metrics_t0 >= 3:
                metrics_t0 = time.time()
                online_n = sum(1 for v in self.online.values() if v)
                self.hub.metrics = {
                    "cameras_online": online_n,
                    "present_count": self.hub.metrics.get("present_count", 0),
                    "events_today": self.hub.metrics.get("events_today", 0),
                    "vision_fps": dict(self.fps),
                }
                self.hub.broadcast_nowait({"type": "metrics", **self.hub.metrics})

    def _session(self) -> Session | None:
        factory = db_session.SessionLocal
        if factory is None:
            return None
        return factory()

    def _infer_frame(self, camera_id: str, frame: np.ndarray) -> list[dict]:
        settings = get_settings()
        engine = get_face_engine()
        gallery = get_gallery()

        # reload gallery if version bumped
        if gallery.version != self._last_gallery_version:
            db = self._session()
            if db is not None:
                try:
                    gallery.load(db)
                finally:
                    db.close()
            self._last_gallery_version = gallery.version

        with self._lock:
            raw_faces = engine.get(frame) if getattr(engine, "ready", False) else []

        h, w = frame.shape[:2]
        dets: list[dict] = []
        for f in raw_faces:
            q = quality_gate(
                f.det_score,
                f.bbox,
                min_det_score=settings.min_det_score,
                min_face_px=settings.min_face_px,
                frame_w=w,
                frame_h=h,
            )
            x1, y1, x2, y2 = f.bbox
            bbox_n = (x1 / w, y1 / h, x2 / w, y2 / h)
            emp_id = None
            label = "LOW QUALITY" if not q.ok else "UNKNOWN"
            score = 0.0
            if q.ok and gallery.size() > 0:
                m = gallery.match(f.embedding)
                emp_id = m.employee_id
                label = m.label
                score = m.score
            dets.append(
                {
                    "bbox": bbox_n,
                    "employee_id": emp_id,
                    "score": score,
                    "label": label,
                    "quality_ok": q.ok,
                    "ts": time.time(),
                    "state": "tracking",
                }
            )

        tracks = assign_tracks(
            self.trackers[camera_id],
            dets,
            iou_threshold=settings.iou_match_threshold,
            max_age=settings.track_max_age_frames,
            vote_window=settings.vote_window,
        )

        out: list[dict] = []
        for tr in tracks:
            commit = None
            if tr.quality_ok and tr.employee_id is not None:
                commit = evaluate_vote(
                    tr,
                    vote_window=settings.vote_window,
                    vote_min_hits=settings.vote_min_hits,
                    min_avg_score=settings.match_threshold,
                )
            if commit is not None:
                db = self._session()
                if db is not None:
                    try:
                        event = commit_identity(
                            db,
                            employee_id=commit.employee_id,
                            camera_id=camera_id,
                            score=commit.avg_score,
                            track_id=tr.track_id,
                            hub=self.hub,
                        )
                        if event:
                            tr.state = "committed"
                            tr.last_commit_ts = time.time()
                            self.hub.metrics["events_today"] = (
                                int(self.hub.metrics.get("events_today") or 0) + 1
                            )
                    finally:
                        db.close()

            out.append(
                {
                    "track_id": tr.track_id,
                    "bbox": list(tr.bbox),
                    "label": tr.label,
                    "employee_id": tr.employee_id,
                    "score": tr.score,
                    "quality_ok": tr.quality_ok,
                    "state": tr.state,
                }
            )
        return out


_worker: VisionWorker | None = None


def get_worker() -> VisionWorker | None:
    return _worker


def start_worker(hub: Any, cameras: list[dict]) -> VisionWorker:
    """Always start the real process pipeline.

    MOCK_VISION uses synthetic frames + MockFaceEngine (FaceEngine double).
    Real mode uses RTSP capture + InsightFace when ready.
    """
    global _worker
    settings = get_settings()
    _worker = VisionWorker(hub)
    synthetic = bool(settings.mock_vision)
    _worker.start_background(cameras, synthetic=synthetic)
    logger.info(
        "VisionWorker started synthetic=%s cameras=%s",
        synthetic,
        [c.get("id") for c in cameras],
    )
    return _worker
