"""Persist attendance events and broadcast."""

from __future__ import annotations

from datetime import date, datetime, timezone
from zoneinfo import ZoneInfo

from sqlalchemy import select
from sqlalchemy.orm import Session

from app.config import get_settings
from app.db.models import AttendanceEvent, Camera, Employee, utcnow
from app.services.attendance.daily import RawEvent, aggregate_daily, daily_csv_headers
from app.services.attendance.fsm import PriorEvent, on_identity_commit


def local_date_for(ts: datetime, tz_name: str) -> date:
    if ts.tzinfo is None:
        ts = ts.replace(tzinfo=timezone.utc)
    return ts.astimezone(ZoneInfo(tz_name)).date()


def last_event_today(db: Session, employee_id: int, local_d: date) -> PriorEvent | None:
    row = db.scalars(
        select(AttendanceEvent)
        .where(
            AttendanceEvent.employee_id == employee_id,
            AttendanceEvent.local_date == local_d,
            AttendanceEvent.kind.in_(["check_in", "check_out"]),
        )
        .order_by(AttendanceEvent.ts.desc())
    ).first()
    if not row:
        return None
    return PriorEvent(kind=row.kind, ts=row.ts, camera_id=row.camera_id)  # type: ignore[arg-type]


def last_camera_event_ts(db: Session, employee_id: int, camera_id: str) -> datetime | None:
    row = db.scalars(
        select(AttendanceEvent)
        .where(
            AttendanceEvent.employee_id == employee_id,
            AttendanceEvent.camera_id == camera_id,
            AttendanceEvent.kind.in_(["check_in", "check_out"]),
        )
        .order_by(AttendanceEvent.ts.desc())
    ).first()
    return row.ts if row else None


def commit_identity(
    db: Session,
    *,
    employee_id: int,
    camera_id: str,
    score: float,
    margin: float | None = None,
    track_id: int | None = None,
    ts: datetime | None = None,
    hub=None,
) -> AttendanceEvent | None:
    settings = get_settings()
    now = ts or utcnow()
    cam = db.get(Camera, camera_id)
    if cam is None:
        return None
    emp = db.get(Employee, employee_id)
    if emp is None or not emp.is_active:
        return None

    local_d = local_date_for(now if now.tzinfo else now.replace(tzinfo=timezone.utc), settings.app_timezone)
    decision = on_identity_commit(
        direction=cam.direction,  # type: ignore[arg-type]
        now=now,
        last_today=last_event_today(db, employee_id, local_d),
        last_same_camera_ts=last_camera_event_ts(db, employee_id, camera_id),
        cooldown_seconds=settings.cooldown_seconds,
        min_dwell_seconds=settings.min_dwell_seconds,
    )
    if decision.action != "commit" or decision.kind is None:
        return None

    event = AttendanceEvent(
        employee_id=employee_id,
        camera_id=camera_id,
        kind=decision.kind,
        score=score,
        margin=margin,
        track_id=track_id,
        ts=now.replace(tzinfo=None) if now.tzinfo else now,
        local_date=local_d,
    )
    db.add(event)
    db.commit()
    db.refresh(event)

    if hub is not None:
        hub.broadcast_nowait(
            {
                "type": "attendance",
                "event_id": event.id,
                "employee_id": employee_id,
                "name": emp.full_name,
                "kind": event.kind,
                "camera_id": camera_id,
                "score": score,
                "ts": event.ts.timestamp() if hasattr(event.ts, "timestamp") else 0,
            }
        )
    return event


def build_daily(db: Session, day: date) -> list[dict]:
    emps = db.scalars(select(Employee).where(Employee.is_active.is_(True))).all()
    employees = [
        {
            "id": e.id,
            "employee_code": e.employee_code,
            "full_name": e.full_name,
            "department": e.department,
        }
        for e in emps
    ]
    evs = db.scalars(select(AttendanceEvent).where(AttendanceEvent.local_date == day)).all()
    raw = [RawEvent(employee_id=e.employee_id or 0, kind=e.kind, ts=e.ts) for e in evs if e.employee_id]
    rows = aggregate_daily(employees, raw)
    return [
        {
            "employee_id": r.employee_id,
            "employee_code": r.employee_code,
            "full_name": r.full_name,
            "department": r.department,
            "first_in": r.first_in.isoformat() + "Z" if r.first_in else None,
            "last_out": r.last_out.isoformat() + "Z" if r.last_out else None,
            "duration_minutes": r.duration_minutes,
            "status": r.status,
            "check_in_count": r.check_in_count,
            "check_out_count": r.check_out_count,
        }
        for r in rows
    ]


def daily_csv(db: Session, day: date) -> str:
    import csv
    import io

    rows = build_daily(db, day)
    buf = io.StringIO()
    w = csv.DictWriter(
        buf,
        fieldnames=daily_csv_headers(),
        extrasaction="ignore",
    )
    w.writeheader()
    for r in rows:
        w.writerow(
            {
                "date": day.isoformat(),
                "employee_code": r["employee_code"],
                "name": r["full_name"],
                "department": r["department"] or "",
                "first_in": r["first_in"] or "",
                "last_out": r["last_out"] or "",
                "duration_minutes": r["duration_minutes"] if r["duration_minutes"] is not None else "",
                "status": r["status"],
                "check_in_count": r["check_in_count"],
                "check_out_count": r["check_out_count"],
            }
        )
    return buf.getvalue()
