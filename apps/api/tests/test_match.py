"""Gallery top1 + margin matching."""

from __future__ import annotations

import numpy as np

from app.services.vision.embed import l2_normalize
from app.services.vision.match import cosine_scores, match_top1


def _ortho_pair():
    a = l2_normalize(np.array([1.0] + [0.0] * 511, dtype=np.float32))
    b = l2_normalize(np.array([0.0, 1.0] + [0.0] * 510, dtype=np.float32))
    return a, b


def test_cosine_identical_is_one():
    a, _ = _ortho_pair()
    scores = cosine_scores(a, a.reshape(1, -1))
    assert abs(float(scores[0]) - 1.0) < 1e-5


def test_match_accepts_above_threshold_with_margin():
    a, b = _ortho_pair()
    gallery = np.stack([a, b])
    q = l2_normalize(a + 0.01 * np.ones(512, dtype=np.float32))
    r = match_top1(q, gallery, [10, 20], ["Alice", "Bob"], threshold=0.4, margin=0.05)
    assert r.employee_id == 10
    assert r.label == "Alice"
    assert r.score >= 0.4
    assert r.margin >= 0.05


def test_match_unknown_below_threshold():
    a, b = _ortho_pair()
    gallery = np.stack([a, b])
    # nearly orthogonal to both
    q = l2_normalize(np.array([0.0, 0.0, 1.0] + [0.0] * 509, dtype=np.float32))
    r = match_top1(q, gallery, [10, 20], ["Alice", "Bob"], threshold=0.45, margin=0.08)
    assert r.employee_id is None
    assert r.label == "UNKNOWN"


def test_match_ambiguous_low_margin():
    a, _ = _ortho_pair()
    # two nearly identical gallery entries
    g1 = a
    g2 = l2_normalize(a + 0.001 * np.ones(512, dtype=np.float32))
    gallery = np.stack([g1, g2])
    r = match_top1(a, gallery, [1, 2], ["A", "B"], threshold=0.3, margin=0.5)
    assert r.label == "AMBIGUOUS"
    assert r.employee_id is None


def test_empty_gallery_unknown():
    q = l2_normalize(np.ones(512, dtype=np.float32))
    r = match_top1(q, np.zeros((0, 512), dtype=np.float32), [], [], 0.45, 0.08)
    assert r.label == "UNKNOWN"
