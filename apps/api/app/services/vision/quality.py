"""Face quality gate — pure functions."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class QualityResult:
    ok: bool
    reason: str | None = None


def quality_gate(
    det_score: float,
    bbox_xyxy: tuple[float, float, float, float],
    *,
    min_det_score: float = 0.5,
    min_face_px: int = 60,
    frame_w: int = 1920,
    frame_h: int = 1080,
    bbox_normalized: bool = False,
) -> QualityResult:
    """
    bbox_xyxy: pixel coords unless bbox_normalized=True (then 0-1 relative to frame).
    """
    if det_score < min_det_score:
        return QualityResult(ok=False, reason="low_det_score")

    x1, y1, x2, y2 = bbox_xyxy
    if bbox_normalized:
        w = (x2 - x1) * frame_w
        h = (y2 - y1) * frame_h
    else:
        w = x2 - x1
        h = y2 - y1

    if min(w, h) < min_face_px:
        return QualityResult(ok=False, reason="face_too_small")

    if w <= 0 or h <= 0:
        return QualityResult(ok=False, reason="invalid_bbox")

    aspect = w / h if h else 0
    if aspect < 0.4 or aspect > 2.5:
        return QualityResult(ok=False, reason="bad_aspect")

    return QualityResult(ok=True, reason=None)
