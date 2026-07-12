"""Environment-driven settings — no buried magic constants."""

from __future__ import annotations

from functools import lru_cache
from pathlib import Path

from pydantic_settings import BaseSettings, SettingsConfigDict

# Repo root: apps/api/app/config.py → parents[3]
REPO_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_DATA_DIR = REPO_ROOT / "data"


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=(".env", str(REPO_ROOT / ".env")),
        env_file_encoding="utf-8",
        extra="ignore",
    )

    # Auth
    admin_password: str = "change-me"
    jwt_secret: str = "dev-jwt-secret-change-me"
    jwt_ttl_hours: int = 12

    # DB / paths
    database_url: str = f"sqlite:///{DEFAULT_DATA_DIR / 'pksp.db'}"
    data_dir: Path = DEFAULT_DATA_DIR  # override via DATA_DIR
    app_timezone: str = "UTC"
    cors_origins: str = "http://localhost:3000"

    # Cameras
    cam_in_rtsp: str = "rtsp://127.0.0.1:8554/demo"
    cam_out_rtsp: str = ""
    cam_in_direction: str = "bidirectional"
    cam_out_direction: str = "out"
    cam_in_webrtc_path: str = "demo"
    cam_out_webrtc_path: str = "cam_out"

    # Vision
    insightface_model: str = "buffalo_l"
    vision_target_fps: float = 5.0
    match_threshold: float = 0.45
    match_margin: float = 0.08
    det_size: int = 640
    enable_antispoof: bool = False
    mock_vision: bool = True  # Phase A theater; set false when buffalo_l ready
    vision_enabled: bool = True
    min_face_px: int = 60
    min_det_score: float = 0.5
    iou_match_threshold: float = 0.3
    track_max_age_frames: int = 10
    vote_window: int = 5
    vote_min_hits: int = 3
    min_enroll_images: int = 1  # demo-friendly; raise for production

    # Attendance
    cooldown_seconds: float = 90.0
    min_dwell_seconds: float = 30.0
    allow_unrecognized_events: bool = True

    # Embedding
    embedding_dim: int = 512
    model_name: str = "buffalo_l"

    @property
    def cors_origin_list(self) -> list[str]:
        return [o.strip() for o in self.cors_origins.split(",") if o.strip()]

    @property
    def resolved_data_dir(self) -> Path:
        p = Path(self.data_dir)
        if not p.is_absolute():
            p = (REPO_ROOT / p).resolve()
        return p

    @property
    def enroll_dir(self) -> Path:
        return self.resolved_data_dir / "enroll"

    @property
    def db_path(self) -> Path:
        url = self.database_url
        if url.startswith("sqlite:///"):
            raw = Path(url.replace("sqlite:///", "", 1))
            if not raw.is_absolute():
                return (REPO_ROOT / raw).resolve()
            return raw
        return self.resolved_data_dir / "pksp.db"

    def resolved_database_url(self) -> str:
        if not self.database_url.startswith("sqlite:///"):
            return self.database_url
        return f"sqlite:///{self.db_path}"


@lru_cache
def get_settings() -> Settings:
    return Settings()
