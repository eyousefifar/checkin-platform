"""Ensure forbidden stack choices are absent from manifests."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def test_no_faiss_postgres_cloud_face_in_deps():
    req = (ROOT / "apps" / "api" / "requirements.txt").read_text().lower()
    pkg = (ROOT / "apps" / "web" / "package.json").read_text().lower()
    forbidden = ["faiss", "psycopg", "postgres", "pgvector", "aws-rekognition", "azure-cognitiveservices-vision-face"]
    blob = req + "\n" + pkg
    for word in forbidden:
        assert word not in blob, f"forbidden dependency mention: {word}"
