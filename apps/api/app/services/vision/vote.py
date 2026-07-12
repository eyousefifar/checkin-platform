"""Temporal voting — commit only after multi-frame agreement."""

from __future__ import annotations

from dataclasses import dataclass

from app.services.vision.track import Track


@dataclass(frozen=True)
class VoteCommit:
    employee_id: int
    avg_score: float
    hits: int


def evaluate_vote(
    track: Track,
    *,
    vote_window: int = 5,
    vote_min_hits: int = 3,
    min_avg_score: float = 0.45,
) -> VoteCommit | None:
    if not track.history:
        return None
    recent = list(track.history)[-vote_window:]
    counts: dict[int, list[float]] = {}
    for v in recent:
        if v.employee_id is None:
            continue
        counts.setdefault(v.employee_id, []).append(v.score)

    if not counts:
        return None

    best_id = max(counts.keys(), key=lambda k: (len(counts[k]), sum(counts[k]) / len(counts[k])))
    scores = counts[best_id]
    if len(scores) < vote_min_hits:
        return None
    avg = sum(scores) / len(scores)
    if avg < min_avg_score:
        return None
    return VoteCommit(employee_id=best_id, avg_score=avg, hits=len(scores))
