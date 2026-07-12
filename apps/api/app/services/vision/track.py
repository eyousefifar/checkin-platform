"""Lightweight IoU tracker — pure functions."""

from __future__ import annotations

from dataclasses import dataclass, field
from collections import deque


def iou(a: tuple[float, float, float, float], b: tuple[float, float, float, float]) -> float:
    ax1, ay1, ax2, ay2 = a
    bx1, by1, bx2, by2 = b
    ix1, iy1 = max(ax1, bx1), max(ay1, by1)
    ix2, iy2 = min(ax2, bx2), min(ay2, by2)
    iw, ih = max(0.0, ix2 - ix1), max(0.0, iy2 - iy1)
    inter = iw * ih
    if inter <= 0:
        return 0.0
    area_a = max(0.0, ax2 - ax1) * max(0.0, ay2 - ay1)
    area_b = max(0.0, bx2 - bx1) * max(0.0, by2 - by1)
    union = area_a + area_b - inter
    return inter / union if union > 0 else 0.0


@dataclass
class TrackVote:
    employee_id: int | None
    score: float
    ts: float


@dataclass
class Track:
    track_id: int
    bbox: tuple[float, float, float, float]
    age: int = 0
    hits: int = 1
    history: deque = field(default_factory=lambda: deque(maxlen=16))
    last_commit_ts: float | None = None
    label: str = "UNKNOWN"
    employee_id: int | None = None
    score: float = 0.0
    quality_ok: bool = True
    state: str = "tracking"


@dataclass
class TrackerState:
    tracks: list[Track] = field(default_factory=list)
    next_id: int = 1


def assign_tracks(
    state: TrackerState,
    detections: list[dict],
    *,
    iou_threshold: float = 0.3,
    max_age: int = 10,
    vote_window: int = 5,
) -> list[Track]:
    """
    detections: list of {bbox, employee_id?, score?, label?, quality_ok?, ts?}
    bbox normalized or absolute — consistent across frames for same camera.
    """
    # age existing
    for t in state.tracks:
        t.age += 1

    unmatched_dets = set(range(len(detections)))
    unmatched_tracks = set(range(len(state.tracks)))
    pairs: list[tuple[int, int, float]] = []

    for ti, tr in enumerate(state.tracks):
        for di, det in enumerate(detections):
            score = iou(tr.bbox, tuple(det["bbox"]))
            if score >= iou_threshold:
                pairs.append((ti, di, score))

    pairs.sort(key=lambda x: -x[2])
    used_t, used_d = set(), set()
    for ti, di, _ in pairs:
        if ti in used_t or di in used_d:
            continue
        used_t.add(ti)
        used_d.add(di)
        unmatched_tracks.discard(ti)
        unmatched_dets.discard(di)
        det = detections[di]
        tr = state.tracks[ti]
        tr.bbox = tuple(det["bbox"])  # type: ignore[assignment]
        tr.age = 0
        tr.hits += 1
        tr.employee_id = det.get("employee_id")
        tr.score = float(det.get("score") or 0.0)
        tr.label = det.get("label") or ("UNKNOWN" if tr.employee_id is None else tr.label)
        tr.quality_ok = bool(det.get("quality_ok", True))
        tr.state = det.get("state") or "tracking"
        if tr.quality_ok:
            tr.history.append(
                TrackVote(
                    employee_id=tr.employee_id,
                    score=tr.score,
                    ts=float(det.get("ts") or 0.0),
                )
            )
            # keep history bounded by vote window*2
            while len(tr.history) > max(vote_window * 2, 8):
                tr.history.popleft()

    # new tracks
    for di in unmatched_dets:
        det = detections[di]
        tr = Track(
            track_id=state.next_id,
            bbox=tuple(det["bbox"]),  # type: ignore[arg-type]
            employee_id=det.get("employee_id"),
            score=float(det.get("score") or 0.0),
            label=det.get("label") or "UNKNOWN",
            quality_ok=bool(det.get("quality_ok", True)),
            state=det.get("state") or "tracking",
            history=deque(maxlen=16),
        )
        if tr.quality_ok:
            tr.history.append(
                TrackVote(
                    employee_id=tr.employee_id,
                    score=tr.score,
                    ts=float(det.get("ts") or 0.0),
                )
            )
        state.next_id += 1
        state.tracks.append(tr)

    # drop old
    state.tracks = [t for t in state.tracks if t.age <= max_age]
    return list(state.tracks)
