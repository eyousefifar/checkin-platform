"""FaceEngine protocol + InsightFace and fixture/mock implementations."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol, runtime_checkable

import numpy as np


@dataclass
class DetectedFace:
    bbox: tuple[float, float, float, float]  # pixel xyxy
    det_score: float
    embedding: np.ndarray


@runtime_checkable
class FaceEngine(Protocol):
    ready: bool
    model_name: str

    def get(self, image_bgr: np.ndarray) -> list[DetectedFace]: ...


class MockFaceEngine:
    """Deterministic engine for tests and Phase A theater enroll path."""

    def __init__(self, dim: int = 512):
        self.ready = True
        self.model_name = "mock"
        self.dim = dim
        # seed-based synthetic embeddings keyed by mean pixel intensity bucket
        self._cache: dict[int, np.ndarray] = {}

    def _vec_for_image(self, image_bgr: np.ndarray) -> np.ndarray:
        # Use average color + size as stable signature for synthetic faces
        mean = float(np.mean(image_bgr)) if image_bgr.size else 0.0
        bucket = int(mean) % 50
        if bucket not in self._cache:
            rng = np.random.default_rng(bucket + 7)
            v = rng.standard_normal(self.dim).astype(np.float32)
            v = v / (np.linalg.norm(v) + 1e-12)
            self._cache[bucket] = v
        # slight per-image noise but same bucket stays matchable
        rng = np.random.default_rng(int(mean * 10) % 1000)
        noise = rng.standard_normal(self.dim).astype(np.float32) * 0.02
        v = self._cache[bucket] + noise
        return (v / (np.linalg.norm(v) + 1e-12)).astype(np.float32)

    def get(self, image_bgr: np.ndarray) -> list[DetectedFace]:
        if image_bgr is None or image_bgr.size == 0:
            return []
        h, w = image_bgr.shape[:2]
        # treat very dark / tiny images as no face
        if min(h, w) < 20 or float(np.mean(image_bgr)) < 5:
            return []
        # single centered face covering most of image
        margin = 0.15
        x1, y1 = w * margin, h * margin
        x2, y2 = w * (1 - margin), h * (1 - margin)
        emb = self._vec_for_image(image_bgr)
        return [
            DetectedFace(
                bbox=(x1, y1, x2, y2),
                det_score=0.99,
                embedding=emb,
            )
        ]


class InsightFaceEngine:
    def __init__(self, model_name: str = "buffalo_l", det_size: int = 640):
        self.model_name = model_name
        self.det_size = det_size
        self.ready = False
        self._app = None
        try:
            from insightface.app import FaceAnalysis

            app = FaceAnalysis(
                name=model_name,
                providers=["CPUExecutionProvider"],
                allowed_modules=["detection", "recognition"],
            )
            app.prepare(ctx_id=-1, det_size=(det_size, det_size))
            self._app = app
            self.ready = True
        except Exception as exc:  # noqa: BLE001 — surface as not ready
            self._error = str(exc)
            self.ready = False

    def get(self, image_bgr: np.ndarray) -> list[DetectedFace]:
        if not self.ready or self._app is None:
            return []
        faces = self._app.get(image_bgr)
        out: list[DetectedFace] = []
        for f in faces:
            bbox = f.bbox.astype(float)
            emb = np.asarray(f.embedding, dtype=np.float32)
            n = np.linalg.norm(emb)
            if n > 0:
                emb = emb / n
            out.append(
                DetectedFace(
                    bbox=(float(bbox[0]), float(bbox[1]), float(bbox[2]), float(bbox[3])),
                    det_score=float(getattr(f, "det_score", 0.0)),
                    embedding=emb,
                )
            )
        return out


_engine: FaceEngine | None = None


def get_face_engine() -> FaceEngine:
    global _engine
    if _engine is not None:
        return _engine
    from app.config import get_settings

    s = get_settings()
    if s.mock_vision:
        _engine = MockFaceEngine(dim=s.embedding_dim)
    else:
        eng = InsightFaceEngine(model_name=s.insightface_model, det_size=s.det_size)
        if not eng.ready:
            # fall back to mock so API stays usable; health reflects vision_ready
            _engine = MockFaceEngine(dim=s.embedding_dim)
            # but mark a degraded real attempt
            object.__setattr__(_engine, "ready", False) if False else None
            _engine = eng  # keep real engine with ready=False
        else:
            _engine = eng
    return _engine


def set_face_engine(engine: FaceEngine | None) -> None:
    global _engine
    _engine = engine
