"""Enrollment API + gallery size — uses MockFaceEngine via real pipeline."""

from __future__ import annotations

from fastapi.testclient import TestClient

from tests.conftest import make_blank_image, make_face_image


def test_enroll_two_identities_gallery_size(client: TestClient, auth_header: dict):
    # create two employees
    r1 = client.post(
        "/api/employees",
        json={"employee_code": "E1001", "full_name": "Alice Demo", "department": "Eng"},
        headers=auth_header,
    )
    assert r1.status_code == 201, r1.text
    id1 = r1.json()["id"]

    r2 = client.post(
        "/api/employees",
        json={"employee_code": "E1002", "full_name": "Bob Demo", "department": "Ops"},
        headers=auth_header,
    )
    assert r2.status_code == 201
    id2 = r2.json()["id"]

    # different seeds → different mock embedding buckets
    img_a = make_face_image(seed=1)
    img_b = make_face_image(seed=2)

    u1 = client.post(
        f"/api/employees/{id1}/images",
        headers=auth_header,
        files=[("files", ("a1.jpg", img_a, "image/jpeg")), ("files", ("a2.jpg", img_a, "image/jpeg"))],
    )
    assert u1.status_code == 200, u1.text
    body1 = u1.json()
    assert body1["usable"] >= 1
    assert body1["embedding_ready"] is True

    u2 = client.post(
        f"/api/employees/{id2}/images",
        headers=auth_header,
        files=[("files", ("b1.jpg", img_b, "image/jpeg"))],
    )
    assert u2.status_code == 200, u2.text
    assert u2.json()["embedding_ready"] is True

    health = client.get("/api/health").json()
    assert health["gallery_size"] >= 2


def test_reject_no_face(client: TestClient, auth_header: dict):
    r = client.post(
        "/api/employees",
        json={"employee_code": "E2001", "full_name": "No Face"},
        headers=auth_header,
    )
    eid = r.json()["id"]
    blank = make_blank_image()
    u = client.post(
        f"/api/employees/{eid}/images",
        headers=auth_header,
        files=[("files", ("dark.jpg", blank, "image/jpeg"))],
    )
    assert u.status_code == 200
    body = u.json()
    assert body["embedding_ready"] is False
    assert body["usable"] == 0
    assert any(x["reason"] == "no_face" for x in body["rejected"])


def test_recompute_embedding(client: TestClient, auth_header: dict):
    r = client.post(
        "/api/employees",
        json={"employee_code": "E3001", "full_name": "Recompute"},
        headers=auth_header,
    )
    eid = r.json()["id"]
    img = make_face_image(seed=5)
    client.post(
        f"/api/employees/{eid}/images",
        headers=auth_header,
        files=[("files", ("c.jpg", img, "image/jpeg"))],
    )
    rc = client.post(f"/api/employees/{eid}/recompute-embedding", headers=auth_header)
    assert rc.status_code == 200
    assert rc.json()["embedding_ready"] is True
