"""Attendance walk-in / walk-out / no double-punch + daily CSV."""

from __future__ import annotations

from datetime import datetime, timedelta
from unittest.mock import patch

from fastapi.testclient import TestClient

from tests.conftest import make_face_image


def _enroll(client: TestClient, headers: dict) -> int:
    r = client.post(
        "/api/employees",
        json={"employee_code": "E9001", "full_name": "Walker", "department": "Demo"},
        headers=headers,
    )
    eid = r.json()["id"]
    img = make_face_image(seed=9)
    client.post(
        f"/api/employees/{eid}/images",
        headers=headers,
        files=[("files", ("w.jpg", img, "image/jpeg"))],
    )
    return eid


def test_walk_in_out_and_cooldown(client: TestClient, auth_header: dict):
    eid = _enroll(client, auth_header)
    t0 = datetime(2026, 7, 12, 8, 0, 0)

    with patch("app.routers.attendance.datetime") as mock_dt:
        mock_dt.utcnow.return_value = t0
        r1 = client.post(
            "/api/attendance/events",
            json={"employee_id": eid, "camera_id": "cam_in", "score": 0.8},
            headers=auth_header,
        )
    assert r1.status_code == 200
    assert r1.json()["ok"] is True
    assert r1.json()["kind"] == "check_in"

    # rapid reappearance under cooldown
    with patch("app.routers.attendance.datetime") as mock_dt:
        mock_dt.utcnow.return_value = t0 + timedelta(seconds=10)
        r2 = client.post(
            "/api/attendance/events",
            json={"employee_id": eid, "camera_id": "cam_in", "score": 0.8},
            headers=auth_header,
        )
    assert r2.json()["ok"] is False

    # walk-out after dwell + cooldown
    with patch("app.routers.attendance.datetime") as mock_dt:
        mock_dt.utcnow.return_value = t0 + timedelta(seconds=120)
        r3 = client.post(
            "/api/attendance/events",
            json={"employee_id": eid, "camera_id": "cam_in", "score": 0.75},
            headers=auth_header,
        )
    assert r3.json()["ok"] is True
    assert r3.json()["kind"] == "check_out"

    daily = client.get("/api/attendance/daily?date=2026-07-12", headers=auth_header)
    assert daily.status_code == 200
    rows = daily.json()
    me = next(x for x in rows if x["employee_id"] == eid)
    assert me["status"] == "present"
    assert me["first_in"] is not None
    assert me["last_out"] is not None
    assert me["check_in_count"] == 1
    assert me["check_out_count"] == 1

    csv_r = client.get("/api/attendance/daily.csv?date=2026-07-12", headers=auth_header)
    assert csv_r.status_code == 200
    text = csv_r.text
    assert "employee_code" in text
    assert "first_in" in text
    assert "E9001" in text
    assert "present" in text
