from __future__ import annotations

from datetime import date, datetime

from fastapi import APIRouter, Depends, Query, Response
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.orm import Session

from app.auth import require_auth
from app.db.models import AttendanceEvent
from app.db.session import get_db
from app.services.attendance.service import build_daily, commit_identity, daily_csv

router = APIRouter(prefix="/attendance", tags=["attendance"])


class ManualEvent(BaseModel):
    employee_id: int
    camera_id: str = "cam_in"
    kind: str | None = None  # if None, FSM decides
    score: float = 1.0


@router.get("/daily")
def daily(
    date_str: str | None = Query(None, alias="date"),
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> list[dict]:
    day = date.fromisoformat(date_str) if date_str else date.today()
    return build_daily(db, day)


@router.get("/events")
def events(
    date_str: str | None = Query(None, alias="date"),
    employee_id: int | None = None,
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> list[dict]:
    stmt = select(AttendanceEvent).order_by(AttendanceEvent.ts.desc())
    if date_str:
        day = date.fromisoformat(date_str)
        stmt = stmt.where(AttendanceEvent.local_date == day)
    if employee_id is not None:
        stmt = stmt.where(AttendanceEvent.employee_id == employee_id)
    rows = db.scalars(stmt.limit(500)).all()
    return [
        {
            "id": r.id,
            "employee_id": r.employee_id,
            "camera_id": r.camera_id,
            "kind": r.kind,
            "score": r.score,
            "ts": r.ts.isoformat() + "Z",
            "local_date": r.local_date.isoformat(),
        }
        for r in rows
    ]


@router.get("/daily.csv")
def daily_csv_export(
    date_str: str | None = Query(None, alias="date"),
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> Response:
    day = date.fromisoformat(date_str) if date_str else date.today()
    content = daily_csv(db, day)
    return Response(
        content=content,
        media_type="text/csv",
        headers={"Content-Disposition": f'attachment; filename="attendance-{day.isoformat()}.csv"'},
    )


@router.post("/events")
def create_event(
    body: ManualEvent,
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> dict:
    event = commit_identity(
        db,
        employee_id=body.employee_id,
        camera_id=body.camera_id,
        score=body.score,
        ts=datetime.utcnow(),
    )
    if event is None:
        return {"ok": False, "detail": "skipped (cooldown or no transition)"}
    return {
        "ok": True,
        "id": event.id,
        "kind": event.kind,
        "local_date": event.local_date.isoformat(),
    }
