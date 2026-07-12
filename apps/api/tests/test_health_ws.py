"""Phase A: health structure + WS typed message shapes."""

from __future__ import annotations

import time

from fastapi.testclient import TestClient


def test_health_returns_structured_json(client: TestClient):
    r = client.get("/api/health")
    assert r.status_code == 200
    body = r.json()
    assert body["status"] == "ok"
    assert "vision_ready" in body
    assert isinstance(body["vision_ready"], bool)
    assert "gallery_size" in body
    assert isinstance(body["gallery_size"], int)
    assert "cameras" in body
    assert isinstance(body["cameras"], list)
    assert any(c["id"] == "cam_in" for c in body["cameras"])


def test_ws_hello_and_detections_shapes(client: TestClient):
    # Real vision worker (synthetic frames + MockFaceEngine) publishes via hub
    with client.websocket_connect("/api/ws/live") as ws:
        hello = ws.receive_json()
        assert hello["type"] == "hello"
        assert "server_ts" in hello
        assert "gallery_version" in hello

        deadline = time.time() + 8
        saw_detections = False
        while time.time() < deadline:
            msg = ws.receive_json()
            if msg.get("type") == "detections":
                assert "camera_id" in msg
                assert "ts" in msg
                assert "frame_w" in msg and "frame_h" in msg
                assert isinstance(msg["faces"], list)
                if msg["faces"]:
                    f = msg["faces"][0]
                    assert "track_id" in f
                    assert "bbox" in f and len(f["bbox"]) == 4
                    assert "label" in f
                    assert "quality_ok" in f
                    assert "state" in f
                saw_detections = True
                break
            if msg.get("type") == "camera_status":
                assert "camera_id" in msg
                assert "online" in msg
        assert saw_detections, "expected detections from vision worker pipeline"
