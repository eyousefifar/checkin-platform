"""Daily attendance aggregate — pure functions."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Iterable, Literal


Status = Literal["absent", "present", "incomplete", "anomaly"]


@dataclass(frozen=True)
class RawEvent:
    employee_id: int
    kind: str
    ts: datetime


@dataclass(frozen=True)
class DailyRow:
    employee_id: int
    employee_code: str
    full_name: str
    department: str | None
    first_in: datetime | None
    last_out: datetime | None
    duration_minutes: int | None
    status: Status
    check_in_count: int
    check_out_count: int


def derive_status(
    first_in: datetime | None,
    last_out: datetime | None,
    check_in_count: int,
    check_out_count: int,
) -> Status:
    if check_in_count == 0 and check_out_count == 0:
        return "absent"
    if check_in_count > 0 and check_out_count == 0:
        return "incomplete"
    if check_in_count > 0 and check_out_count > 0:
        return "present"
    if check_out_count > 0 and check_in_count == 0:
        return "anomaly"
    return "absent"


def aggregate_daily(
    employees: Iterable[dict],
    events: Iterable[RawEvent],
) -> list[DailyRow]:
    """
    employees: {id, employee_code, full_name, department}
    events: only for the target local_date
    """
    by_emp: dict[int, list[RawEvent]] = {}
    for ev in events:
        if ev.kind not in ("check_in", "check_out"):
            continue
        by_emp.setdefault(ev.employee_id, []).append(ev)

    rows: list[DailyRow] = []
    for emp in employees:
        eid = emp["id"]
        evs = sorted(by_emp.get(eid, []), key=lambda e: e.ts)
        ins = [e for e in evs if e.kind == "check_in"]
        outs = [e for e in evs if e.kind == "check_out"]
        first_in = ins[0].ts if ins else None
        last_out = outs[-1].ts if outs else None
        duration = None
        if first_in and last_out and last_out >= first_in:
            duration = int((last_out - first_in).total_seconds() // 60)
        status = derive_status(first_in, last_out, len(ins), len(outs))
        rows.append(
            DailyRow(
                employee_id=eid,
                employee_code=emp["employee_code"],
                full_name=emp["full_name"],
                department=emp.get("department"),
                first_in=first_in,
                last_out=last_out,
                duration_minutes=duration,
                status=status,
                check_in_count=len(ins),
                check_out_count=len(outs),
            )
        )
    rows.sort(key=lambda r: r.full_name.lower())
    return rows


def daily_csv_headers() -> list[str]:
    return [
        "date",
        "employee_code",
        "name",
        "department",
        "first_in",
        "last_out",
        "duration_minutes",
        "status",
        "check_in_count",
        "check_out_count",
    ]
