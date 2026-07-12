"""Attendance FSM + cooldown — pure logic, single module."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from enum import Enum
from typing import Literal


EventKind = Literal["check_in", "check_out", "unrecognized", "rejected_spoof", "rejected_low_conf"]
Direction = Literal["in", "out", "bidirectional"]


class SkipReason(str, Enum):
    COOLDOWN = "cooldown"
    NO_TRANSITION = "no_transition"
    MIN_DWELL = "min_dwell"


@dataclass(frozen=True)
class PriorEvent:
    kind: EventKind
    ts: datetime
    camera_id: str


@dataclass(frozen=True)
class FsmDecision:
    action: Literal["commit", "skip"]
    kind: EventKind | None = None
    reason: str | None = None


def _as_utc(ts: datetime) -> datetime:
    if ts.tzinfo is None:
        return ts.replace(tzinfo=timezone.utc)
    return ts.astimezone(timezone.utc)


def in_cooldown(
    last_same_camera_ts: datetime | None,
    now: datetime,
    cooldown_seconds: float,
) -> bool:
    if last_same_camera_ts is None:
        return False
    delta = (_as_utc(now) - _as_utc(last_same_camera_ts)).total_seconds()
    return delta < cooldown_seconds


def resolve_kind(
    direction: Direction,
    last_today: PriorEvent | None,
    now: datetime,
    *,
    min_dwell_seconds: float = 30.0,
) -> EventKind | None:
    if direction == "in":
        return "check_in"
    if direction == "out":
        return "check_out"

    # bidirectional
    if last_today is None or last_today.kind == "check_out":
        return "check_in"
    if last_today.kind == "check_in":
        dwell = (_as_utc(now) - _as_utc(last_today.ts)).total_seconds()
        if dwell >= min_dwell_seconds:
            return "check_out"
        return None
    return "check_in"


def on_identity_commit(
    *,
    direction: Direction,
    now: datetime,
    last_today: PriorEvent | None,
    last_same_camera_ts: datetime | None,
    cooldown_seconds: float = 90.0,
    min_dwell_seconds: float = 30.0,
) -> FsmDecision:
    if in_cooldown(last_same_camera_ts, now, cooldown_seconds):
        return FsmDecision(action="skip", reason=SkipReason.COOLDOWN.value)

    kind = resolve_kind(direction, last_today, now, min_dwell_seconds=min_dwell_seconds)
    if kind is None:
        return FsmDecision(action="skip", reason=SkipReason.NO_TRANSITION.value)
    return FsmDecision(action="commit", kind=kind)
