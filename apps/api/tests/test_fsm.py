"""Attendance FSM: in/out/bidirectional + cooldown."""

from __future__ import annotations

from datetime import datetime, timedelta, timezone

from app.services.attendance.fsm import PriorEvent, in_cooldown, on_identity_commit, resolve_kind


def ts(h: int, m: int = 0) -> datetime:
    return datetime(2026, 7, 12, h, m, tzinfo=timezone.utc)


def test_in_camera_always_check_in():
    d = on_identity_commit(
        direction="in",
        now=ts(8),
        last_today=None,
        last_same_camera_ts=None,
        cooldown_seconds=90,
    )
    assert d.action == "commit"
    assert d.kind == "check_in"


def test_out_camera_always_check_out():
    d = on_identity_commit(
        direction="out",
        now=ts(17),
        last_today=PriorEvent(kind="check_in", ts=ts(8), camera_id="cam_in"),
        last_same_camera_ts=None,
        cooldown_seconds=90,
    )
    assert d.action == "commit"
    assert d.kind == "check_out"


def test_bidirectional_walk_in_then_out_after_dwell():
    d1 = on_identity_commit(
        direction="bidirectional",
        now=ts(8),
        last_today=None,
        last_same_camera_ts=None,
        min_dwell_seconds=30,
    )
    assert d1.kind == "check_in"

    d2 = on_identity_commit(
        direction="bidirectional",
        now=ts(8, 1),  # 60s later
        last_today=PriorEvent(kind="check_in", ts=ts(8), camera_id="cam_in"),
        last_same_camera_ts=ts(8),
        cooldown_seconds=30,  # allow after 30s camera cooldown for this unit
        min_dwell_seconds=30,
    )
    # last_same_camera 60s ago with cooldown 30 → ok
    assert d2.action == "commit"
    assert d2.kind == "check_out"


def test_cooldown_blocks_double_punch():
    now = ts(8, 1)
    last = now - timedelta(seconds=10)
    d = on_identity_commit(
        direction="in",
        now=now,
        last_today=PriorEvent(kind="check_in", ts=last, camera_id="cam_in"),
        last_same_camera_ts=last,
        cooldown_seconds=90,
    )
    assert d.action == "skip"
    assert d.reason == "cooldown"


def test_in_cooldown_helper():
    now = datetime.now(timezone.utc)
    assert in_cooldown(now - timedelta(seconds=10), now, 90) is True
    assert in_cooldown(now - timedelta(seconds=100), now, 90) is False
    assert in_cooldown(None, now, 90) is False


def test_bidirectional_no_transition_before_dwell():
    kind = resolve_kind(
        "bidirectional",
        PriorEvent(kind="check_in", ts=ts(8), camera_id="c"),
        ts(8, 0) + timedelta(seconds=10),
        min_dwell_seconds=30,
    )
    assert kind is None
