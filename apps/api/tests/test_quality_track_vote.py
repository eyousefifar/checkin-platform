"""Quality gate, IoU assign, vote commit — pure logic."""

from __future__ import annotations

from app.services.vision.quality import quality_gate
from app.services.vision.track import TrackerState, assign_tracks, iou
from app.services.vision.vote import evaluate_vote


def test_quality_rejects_low_score():
    r = quality_gate(0.2, (100, 100, 200, 200), min_det_score=0.5)
    assert not r.ok
    assert r.reason == "low_det_score"


def test_quality_rejects_small_face():
    r = quality_gate(0.9, (10, 10, 40, 40), min_face_px=60)
    assert not r.ok
    assert r.reason == "face_too_small"


def test_quality_accepts_good_face():
    r = quality_gate(0.9, (100, 100, 220, 240), min_face_px=60)
    assert r.ok


def test_quality_normalized_bbox():
    r = quality_gate(
        0.9,
        (0.4, 0.3, 0.55, 0.55),
        min_face_px=60,
        frame_w=1920,
        frame_h=1080,
        bbox_normalized=True,
    )
    assert r.ok


def test_iou_identical():
    box = (0.1, 0.1, 0.5, 0.5)
    assert abs(iou(box, box) - 1.0) < 1e-6


def test_iou_disjoint():
    assert iou((0, 0, 0.1, 0.1), (0.5, 0.5, 0.6, 0.6)) == 0.0


def test_assign_tracks_reuses_id():
    state = TrackerState()
    t1 = assign_tracks(
        state,
        [{"bbox": (0.1, 0.1, 0.3, 0.4), "employee_id": 1, "score": 0.7, "label": "A", "quality_ok": True, "ts": 1.0}],
    )
    assert len(t1) == 1
    tid = t1[0].track_id
    t2 = assign_tracks(
        state,
        [{"bbox": (0.12, 0.11, 0.32, 0.41), "employee_id": 1, "score": 0.72, "label": "A", "quality_ok": True, "ts": 1.1}],
    )
    assert len(t2) == 1
    assert t2[0].track_id == tid
    assert len(t2[0].history) >= 2


def test_vote_commits_after_min_hits():
    state = TrackerState()
    for i in range(5):
        tracks = assign_tracks(
            state,
            [
                {
                    "bbox": (0.1 + i * 0.01, 0.1, 0.3 + i * 0.01, 0.4),
                    "employee_id": 7,
                    "score": 0.6,
                    "label": "X",
                    "quality_ok": True,
                    "ts": float(i),
                }
            ],
            vote_window=5,
        )
    commit = evaluate_vote(tracks[0], vote_window=5, vote_min_hits=3, min_avg_score=0.45)
    assert commit is not None
    assert commit.employee_id == 7
    assert commit.hits >= 3


def test_vote_no_commit_insufficient_hits():
    state = TrackerState()
    tracks = assign_tracks(
        state,
        [{"bbox": (0.1, 0.1, 0.3, 0.4), "employee_id": 7, "score": 0.6, "label": "X", "quality_ok": True, "ts": 1.0}],
    )
    commit = evaluate_vote(tracks[0], vote_window=5, vote_min_hits=3, min_avg_score=0.45)
    assert commit is None
