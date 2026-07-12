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

    # Real camera integration (highest quality feed) - populated from .env
    # When ip_camera_user + ip_camera_pass are set, effective_cam_in_rtsp builds
    # the authenticated high-quality URL (stream1 = 2560x1440 H.265 on this camera).
    ip_camera_user: str = ""
    ip_camera_pass: str = ""
    cam_ip: str = "10.39.45.167"
    cam_high_quality_path: str = "stream1"

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

    # Vision acceleration (Intel Linux + OpenVINO / VAAPI focused; safe defaults)
    onnx_providers: str = "CPUExecutionProvider"  # comma-separated, order matters. e.g. "OpenVINOExecutionProvider,CPUExecutionProvider"
    onnx_intra_op_num_threads: int = 0  # 0=let runtime decide; set 4-8 on hybrid CPUs
    onnx_inter_op_num_threads: int = 0
    capture_backend: str = "auto"  # auto | opencv_ffmpeg | ffmpeg_vaapi
    vision_adaptive: bool = False

    # Note: the high-quality camera integration (2560x1440 H.265) benefits greatly
    # from CAPTURE_BACKEND=auto (VAAPI decode + scale) on this hardware.

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
    def onnx_providers_list(self) -> list[str]:
        """Parsed list for FaceAnalysis providers= kwarg. Strips whitespace."""
        raw = (self.onnx_providers or "CPUExecutionProvider").strip()
        parts = [p.strip() for p in raw.split(",") if p.strip()]
        return parts or ["CPUExecutionProvider"]

    @property
    def effective_cam_in_rtsp(self) -> str:
        """Highest-quality RTSP URL for cam_in.

        If CAM_IN_RTSP already contains auth (@), use it verbatim (override).
        Otherwise, when IP_CAMERA_USER/PASS are present in .env, build the
        authenticated high-quality stream (2560x1440 H.265 on this camera).
        """
        if self.cam_in_rtsp and "@" in self.cam_in_rtsp:
            return self.cam_in_rtsp
        if self.ip_camera_user and self.ip_camera_pass:
            auth = f"{self.ip_camera_user}:{self.ip_camera_pass}@"
            return f"rtsp://{auth}{self.cam_ip}:554/{self.cam_high_quality_path}"
        return self.cam_in_rtsp

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
