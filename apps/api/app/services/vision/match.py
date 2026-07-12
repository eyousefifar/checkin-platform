"""NumPy cosine gallery matching — not FAISS."""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

from app.services.vision.embed import l2_normalize


@dataclass(frozen=True)
class MatchResult:
    employee_id: int | None
    score: float
    margin: float
    label: str  # identity name, UNKNOWN, or AMBIGUOUS


def cosine_scores(query: np.ndarray, gallery: np.ndarray) -> np.ndarray:
    """gallery shape (N, D), query (D,). Both should be L2-normalized."""
    q = l2_normalize(query)
    g = np.asarray(gallery, dtype=np.float32)
    if g.ndim != 2 or g.shape[0] == 0:
        return np.zeros((0,), dtype=np.float32)
    # defensive row-normalize
    norms = np.linalg.norm(g, axis=1, keepdims=True)
    norms = np.maximum(norms, 1e-12)
    g = g / norms
    return (g @ q).astype(np.float32)


def match_top1(
    query: np.ndarray,
    gallery: np.ndarray,
    employee_ids: list[int],
    names: list[str],
    threshold: float,
    margin: float,
) -> MatchResult:
    if gallery is None or len(employee_ids) == 0 or gallery.shape[0] == 0:
        return MatchResult(employee_id=None, score=0.0, margin=0.0, label="UNKNOWN")

    scores = cosine_scores(query, gallery)
    order = np.argsort(-scores)
    top1 = int(order[0])
    top1_score = float(scores[top1])
    top2_score = float(scores[order[1]]) if len(order) > 1 else -1.0
    m = top1_score - top2_score if len(order) > 1 else top1_score

    if top1_score < threshold:
        return MatchResult(employee_id=None, score=top1_score, margin=m, label="UNKNOWN")
    if m < margin and len(order) > 1:
        return MatchResult(employee_id=None, score=top1_score, margin=m, label="AMBIGUOUS")
    return MatchResult(
        employee_id=employee_ids[top1],
        score=top1_score,
        margin=m,
        label=names[top1],
    )
