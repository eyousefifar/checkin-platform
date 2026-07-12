"""Daily aggregate statuses."""

from __future__ import annotations

from datetime import datetime

from app.services.attendance.daily import RawEvent, aggregate_daily, daily_csv_headers, derive_status


def test_derive_status_matrix():
    t1 = datetime(2026, 7, 12, 8)
    t2 = datetime(2026, 7, 12, 17)
    assert derive_status(None, None, 0, 0) == "absent"
    assert derive_status(t1, None, 1, 0) == "incomplete"
    assert derive_status(t1, t2, 1, 1) == "present"
    assert derive_status(None, t2, 0, 1) == "anomaly"


def test_aggregate_daily_rows():
    emps = [
        {"id": 1, "employee_code": "E1", "full_name": "Alice", "department": "Eng"},
        {"id": 2, "employee_code": "E2", "full_name": "Bob", "department": None},
    ]
    events = [
        RawEvent(1, "check_in", datetime(2026, 7, 12, 8, 0)),
        RawEvent(1, "check_out", datetime(2026, 7, 12, 17, 0)),
        RawEvent(2, "check_in", datetime(2026, 7, 12, 9, 0)),
    ]
    rows = aggregate_daily(emps, events)
    by_code = {r.employee_code: r for r in rows}
    assert by_code["E1"].status == "present"
    assert by_code["E1"].duration_minutes == 540
    assert by_code["E1"].check_in_count == 1
    assert by_code["E1"].check_out_count == 1
    assert by_code["E2"].status == "incomplete"
    assert by_code["E2"].last_out is None


def test_csv_headers():
    h = daily_csv_headers()
    assert h[0] == "date"
    assert "first_in" in h
    assert "status" in h
    assert "check_out_count" in h
