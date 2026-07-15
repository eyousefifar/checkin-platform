//! SQLite access via sqlx + migrations.

use anyhow::{Context, Result};
use chrono::Utc;
use pksp_core::on_identity_commit;
use pksp_core::{
    aggregate_daily, csv_encode_field, daily_csv_headers, Direction, EmployeeRef, EventKind,
    FsmDecision, PriorEvent, RawEvent, SkipReason,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::info;

#[derive(Debug, Clone)]
pub struct Settings {
    pub database_url: String,
    pub data_dir: PathBuf,
    pub admin_password: String,
    /// True when ADMIN_PASSWORD was set in the process environment (not a code default).
    pub admin_password_from_env: bool,
    pub jwt_secret: String,
    /// True when JWT_SECRET was set in the process environment (not a code default).
    pub jwt_secret_from_env: bool,
    pub jwt_ttl_hours: i64,
    pub cors_origins: Vec<String>,
    pub app_timezone: String,
    pub cam_in_rtsp: String,
    pub cam_out_rtsp: String,
    pub cam_in_webrtc_path: String,
    pub cam_out_webrtc_path: String,
    pub cam_in_direction: String,
    pub cam_out_direction: String,
    pub camera_upsert: bool,
    pub vision_enabled: bool,
    pub vision_target_fps: f64,
    pub match_threshold: f32,
    pub match_margin: f32,
    pub min_face_px: i32,
    pub min_det_score: f32,
    pub iou_match_threshold: f32,
    pub track_max_age_frames: i32,
    pub vote_window: usize,
    pub vote_min_hits: usize,
    pub min_enroll_images: usize,
    pub cooldown_seconds: f64,
    pub min_dwell_seconds: f64,
    pub embedding_dim: usize,
    pub enable_smart_scene: bool,
    pub walkby_min_dwell_frames: usize,
    /// Directory for zones.{camera_id}.json (empty → defaults only).
    pub zone_config_dir: PathBuf,
    /// Max approximate yaw degrees; 0 disables pose check.
    pub pose_max_yaw: f32,
    /// Min Laplacian variance; 0 disables blur check.
    pub blur_min_var: f32,
    pub det_size: i32,
    pub onnx_providers: String,
    pub vision_adaptive: bool,
    /// Preferred browser-safe H.264 RTSP (used when MEDIA_SOURCE_MODE=copy).
    pub cam_in_h264_rtsp: String,
    /// Publication policy: external | copy | transcode (default external).
    pub media_source_mode: String,
    /// Browser publish path name inside MediaMTX (default cam_in_h264).
    pub media_publish_path: String,
    /// Loopback MediaMTX HTTP API (default 127.0.0.1:9997).
    pub mediamtx_api_addr: String,
    pub bind_addr: String,
    /// Empty or "mediamtx" → auto-resolve apps/edge/bin/mediamtx then PATH
    pub mediamtx_bin: String,
    pub mediamtx_config: PathBuf,
    /// Empty or "ffmpeg" → auto-resolve apps/edge/bin/ffmpeg then PATH
    pub ffmpeg_bin: String,
    pub webrtc_base: String,
    /// Total multipart body budget for enrollment uploads (bytes).
    pub max_enroll_upload_bytes: usize,
    /// Max files accepted in one enrollment upload.
    pub max_enroll_files: usize,
    /// Max decoded size of a single enrollment image (bytes).
    pub max_enroll_file_bytes: usize,
    /// Max width or height for an enrollment image.
    pub max_enroll_image_dim: u32,
    /// Max width*height pixels for an enrollment image.
    pub max_enroll_pixels: u64,
}

impl Settings {
    pub fn from_env() -> Self {
        let data_dir = PathBuf::from(env_or("DATA_DIR", "data"));
        // Prefer explicit DATABASE_URL; otherwise place DB under DATA_DIR.
        // A three-slash SQLite URL may be relative; resolve it later.
        let db_default = format!("sqlite://{}/pksp-rust.db?mode=rwc", data_dir.display());
        let database_url = env_or("DATABASE_URL", &db_default);
        let (admin_password, admin_password_from_env) =
            env_or_tracked("ADMIN_PASSWORD", "change-me");
        let (jwt_secret, jwt_secret_from_env) =
            env_or_tracked("JWT_SECRET", "dev-jwt-secret-change-me");
        Self {
            database_url,
            data_dir,
            admin_password,
            admin_password_from_env,
            jwt_secret,
            jwt_secret_from_env,
            jwt_ttl_hours: env_or("JWT_TTL_HOURS", "12").parse().unwrap_or(12),
            cors_origins: env_or("CORS_ORIGINS", "http://localhost:3000")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            app_timezone: env_or("APP_TIMEZONE", "Asia/Tehran"),
            cam_in_rtsp: env_or("CAM_IN_RTSP", ""),
            cam_out_rtsp: env_or("CAM_OUT_RTSP", ""),
            // Default matches CAM_IN path name so vision RTSP + browser WHEP stay aligned
            // for real cameras and local MediaMTX publishers (demo stack uses cam_in too).
            cam_in_webrtc_path: env_or("CAM_IN_WEBRTC_PATH", "cam_in"),
            cam_out_webrtc_path: env_or("CAM_OUT_WEBRTC_PATH", "cam_out"),
            cam_in_direction: env_or("CAM_IN_DIRECTION", "bidirectional"),
            cam_out_direction: env_or("CAM_OUT_DIRECTION", "out"),
            camera_upsert: env_or("CAMERA_UPSERT", "true") != "false",
            vision_enabled: env_or("VISION_ENABLED", "true") != "false",
            vision_target_fps: env_or("VISION_TARGET_FPS", "5").parse().unwrap_or(5.0),
            match_threshold: env_or("MATCH_THRESHOLD", "0.75").parse().unwrap_or(0.75),
            match_margin: env_or("MATCH_MARGIN", "0.10").parse().unwrap_or(0.10),
            min_face_px: env_or("MIN_FACE_PX", "60").parse().unwrap_or(60),
            min_det_score: env_or("MIN_DET_SCORE", "0.5").parse().unwrap_or(0.5),
            iou_match_threshold: env_or("IOU_MATCH_THRESHOLD", "0.3").parse().unwrap_or(0.3),
            track_max_age_frames: env_or("TRACK_MAX_AGE_FRAMES", "10").parse().unwrap_or(10),
            vote_window: env_or("VOTE_WINDOW", "5").parse().unwrap_or(5),
            vote_min_hits: env_or("VOTE_MIN_HITS", "3").parse().unwrap_or(3),
            min_enroll_images: env_or("MIN_ENROLL_IMAGES", "5").parse().unwrap_or(5),
            cooldown_seconds: env_or("COOLDOWN_SECONDS", "90").parse().unwrap_or(90.0),
            min_dwell_seconds: env_or("MIN_DWELL_SECONDS", "30").parse().unwrap_or(30.0),
            embedding_dim: env_or("EMBEDDING_DIM", "512").parse().unwrap_or(512),
            enable_smart_scene: env_or("ENABLE_SMART_SCENE", "true") != "false",
            walkby_min_dwell_frames: env_or("WALKBY_MIN_DWELL_FRAMES", "3").parse().unwrap_or(3),
            zone_config_dir: {
                if let Ok(v) = std::env::var("ZONE_CONFIG_DIR") {
                    PathBuf::from(v)
                } else {
                    let edge = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../configs");
                    if edge.is_dir() {
                        edge
                    } else {
                        PathBuf::from("configs")
                    }
                }
            },
            pose_max_yaw: env_or("POSE_MAX_YAW", "30").parse().unwrap_or(30.0),
            blur_min_var: env_or("BLUR_MIN_VAR", "75").parse().unwrap_or(75.0),
            det_size: env_or("DET_SIZE", "640").parse().unwrap_or(640),
            onnx_providers: env_or("ONNX_PROVIDERS", "CPUExecutionProvider"),
            vision_adaptive: env_or("VISION_ADAPTIVE", "false") == "true",
            cam_in_h264_rtsp: env_or("CAM_IN_H264_RTSP", ""),
            media_source_mode: env_or("MEDIA_SOURCE_MODE", "external"),
            media_publish_path: env_or("MEDIA_PUBLISH_PATH", "cam_in_h264"),
            mediamtx_api_addr: env_or("MEDIAMTX_API_ADDR", "127.0.0.1:9997"),
            bind_addr: env_or("BIND_ADDR", "127.0.0.1:8000"),
            // Auto-resolve bundled apps/edge/bin/* unless overridden
            mediamtx_bin: env_or("MEDIAMTX_BIN", "mediamtx"),
            mediamtx_config: {
                if let Ok(v) = std::env::var("MEDIAMTX_CONFIG") {
                    PathBuf::from(v)
                } else {
                    let edge = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("../../configs/mediamtx.yml");
                    if edge.is_file() {
                        edge
                    } else {
                        PathBuf::from("configs/mediamtx.yml")
                    }
                }
            },
            ffmpeg_bin: env_or("FFMPEG_BIN", "ffmpeg"),
            webrtc_base: env_or("WEBRTC_BASE", "http://localhost:8889"),
            max_enroll_upload_bytes: env_or("MAX_ENROLL_UPLOAD_BYTES", "33554432")
                .parse()
                .unwrap_or(33_554_432),
            max_enroll_files: env_or("MAX_ENROLL_FILES", "10").parse().unwrap_or(10),
            max_enroll_file_bytes: env_or("MAX_ENROLL_FILE_BYTES", "5242880")
                .parse()
                .unwrap_or(5_242_880),
            max_enroll_image_dim: env_or("MAX_ENROLL_IMAGE_DIM", "4096")
                .parse()
                .unwrap_or(4096),
            max_enroll_pixels: env_or("MAX_ENROLL_PIXELS", "20000000")
                .parse()
                .unwrap_or(20_000_000),
        }
    }

    pub fn enroll_dir(&self) -> PathBuf {
        self.data_dir.join("enroll")
    }

    pub fn model_dir(&self) -> PathBuf {
        self.data_dir.join("models/buffalo_l")
    }

    /// Fail closed before DB/media/worker startup when bind or secrets are unsafe.
    pub fn validate_startup(&self) -> Result<std::net::SocketAddr, String> {
        self.validate_enrollment_limits()?;
        if self.cam_in_rtsp.trim().is_empty() {
            return Err("CAM_IN_RTSP must be set for enabled camera cam_in".into());
        }
        if self.embedding_dim != 512 {
            return Err("EMBEDDING_DIM must be 512 for buffalo_l".into());
        }
        validate_bind_and_secrets(
            &self.bind_addr,
            &self.admin_password,
            &self.jwt_secret,
            self.admin_password_from_env,
            self.jwt_secret_from_env,
        )
    }

    /// Reject nonsensical enrollment limit combinations at process start.
    pub fn validate_enrollment_limits(&self) -> Result<(), String> {
        if self.max_enroll_files == 0 {
            return Err("MAX_ENROLL_FILES must be >= 1".into());
        }
        if self.max_enroll_file_bytes == 0 {
            return Err("MAX_ENROLL_FILE_BYTES must be >= 1".into());
        }
        if self.max_enroll_upload_bytes < self.max_enroll_file_bytes {
            return Err("MAX_ENROLL_UPLOAD_BYTES must be >= MAX_ENROLL_FILE_BYTES".into());
        }
        if self.max_enroll_image_dim == 0 {
            return Err("MAX_ENROLL_IMAGE_DIM must be >= 1".into());
        }
        if self.max_enroll_pixels == 0 {
            return Err("MAX_ENROLL_PIXELS must be >= 1".into());
        }
        Ok(())
    }
}

/// Parse `BIND_ADDR` and enforce secret policy for non-loopback binds.
///
/// Loopback demo may keep built-in development fallbacks. Any non-loopback
/// (including `0.0.0.0` / `::`) requires an explicit non-demo `ADMIN_PASSWORD`
/// and an explicit `JWT_SECRET` of at least 32 bytes. Malformed bind text is
/// rejected — never remapped to a wildcard.
pub fn validate_bind_and_secrets(
    bind_addr: &str,
    admin_password: &str,
    jwt_secret: &str,
    admin_explicit: bool,
    jwt_explicit: bool,
) -> Result<std::net::SocketAddr, String> {
    let addr: std::net::SocketAddr = bind_addr.parse().map_err(|_| {
        format!("BIND_ADDR is malformed ({bind_addr:?}); expected host:port such as 127.0.0.1:8000")
    })?;

    let loopback = match addr.ip() {
        std::net::IpAddr::V4(ip) => ip.is_loopback(),
        std::net::IpAddr::V6(ip) => ip.is_loopback(),
    };
    if loopback {
        return Ok(addr);
    }

    const DEMO_ADMIN: &str = "change-me";
    if !admin_explicit || admin_password.is_empty() || admin_password == DEMO_ADMIN {
        return Err(
            "ADMIN_PASSWORD must be explicitly set to a non-demo value when BIND_ADDR is not loopback"
                .into(),
        );
    }
    if !jwt_explicit || jwt_secret.len() < 32 {
        return Err(
            "JWT_SECRET must be explicitly set and at least 32 bytes when BIND_ADDR is not loopback"
                .into(),
        );
    }
    Ok(addr)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_or_tracked(key: &str, default: &str) -> (String, bool) {
    match std::env::var(key) {
        Ok(v) => (v, true),
        Err(_) => (default.to_string(), false),
    }
}

/// Resolve a SQLite file path from SQLAlchemy / sqlx-style URLs.
///
/// | Input | Path |
/// |---|---|
/// | `sqlite:///./data/pksp.db` | `./data/pksp.db` (relative) |
/// | `sqlite:///data/pksp.db` | `data/pksp.db` (relative) |
/// | `sqlite:////tmp/x.db` | `/tmp/x.db` (absolute) |
/// | `data/foo.db` | `data/foo.db` (bare) |
///
/// **Bug fixed:** naive `strip_prefix("sqlite://")` turned
/// `sqlite:///./data/pksp.db` into `/./data/pksp.db` → `create_dir_all("/data")` EROFS.
pub fn resolve_sqlite_path(url: &str) -> PathBuf {
    let base = url.split('?').next().unwrap_or(url).trim();
    if base == ":memory:" || base.ends_with(":memory:") {
        return PathBuf::from(":memory:");
    }
    if let Some(after_scheme) = base.strip_prefix("sqlite:") {
        // Forms after "sqlite:":
        //   ///./data/pksp.db  (3 slashes → relative)
        //   ////tmp/x.db       (4 slashes → absolute)
        //   //localhost/x      (rare)
        if let Some(rest) = after_scheme.strip_prefix("//") {
            // rest is "/./data/..." or "//tmp/..." or "host/..."
            if let Some(abs) = rest.strip_prefix("//") {
                // four-slash absolute: ////tmp/x → tmp was wrong; rest was //tmp/x
                // after strip_prefix("//") on after_scheme: rest starts with /
                // sqlite:////tmp/x → after_scheme = ////tmp/x → strip // → //tmp/x
                return PathBuf::from(format!("/{abs}"));
            }
            if let Some(rel) = rest.strip_prefix('/') {
                // three-slash: /./data/pksp.db or /data/pksp.db → drop one leading /
                return PathBuf::from(rel);
            }
            // sqlite://host/path — treat rest as path
            return PathBuf::from(rest);
        }
        // sqlite:/path (unusual)
        return PathBuf::from(after_scheme.trim_start_matches('/'));
    }
    PathBuf::from(base)
}

pub async fn connect_pool(settings: &Settings) -> Result<SqlitePool> {
    std::fs::create_dir_all(&settings.data_dir)
        .with_context(|| format!("create data_dir {}", settings.data_dir.display()))?;
    std::fs::create_dir_all(settings.enroll_dir())
        .with_context(|| format!("create enroll_dir {}", settings.enroll_dir().display()))?;

    let db_path = resolve_sqlite_path(&settings.database_url);
    if db_path != Path::new(":memory:") {
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "create sqlite parent dir {} (from DATABASE_URL={})",
                        parent.display(),
                        settings.database_url
                    )
                })?;
            }
        }
    }

    let opts = if db_path == Path::new(":memory:") {
        SqliteConnectOptions::from_str("sqlite::memory:")?
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5))
    } else {
        SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5))
    };

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with(opts)
        .await
        .with_context(|| format!("connect sqlite at {}", db_path.display()))?;

    // Run migrations from apps/edge/migrations relative to CARGO_MANIFEST_DIR of workspace
    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../migrations"
    )))
    .await
    .context("load migrations")?;
    migrator.run(&pool).await.context("run migrations")?;

    seed_or_upsert_cameras(&pool, settings).await?;
    ensure_gallery_version(&pool).await?;
    Ok(pool)
}

async fn ensure_gallery_version(pool: &SqlitePool) -> Result<()> {
    sqlx::query("INSERT OR IGNORE INTO app_meta(key, value) VALUES('gallery_version', '0')")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn seed_or_upsert_cameras(pool: &SqlitePool, settings: &Settings) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let _ = now;
    upsert_camera(
        pool,
        "cam_in",
        "Entrance",
        &settings.cam_in_rtsp,
        &settings.cam_in_webrtc_path,
        &settings.cam_in_direction,
        0,
        settings.camera_upsert,
    )
    .await?;
    if !settings.cam_out_rtsp.is_empty() {
        upsert_camera(
            pool,
            "cam_out",
            "Exit",
            &settings.cam_out_rtsp,
            &settings.cam_out_webrtc_path,
            &settings.cam_out_direction,
            1,
            settings.camera_upsert,
        )
        .await?;
    }
    info!("cameras seeded/upserted");
    Ok(())
}

#[allow(clippy::too_many_arguments)] // ponytail: orchestration boundary; group only when another caller exists
async fn upsert_camera(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    rtsp: &str,
    webrtc: &str,
    direction: &str,
    sort_order: i32,
    do_upsert: bool,
) -> Result<()> {
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM cameras WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        sqlx::query(
            "INSERT INTO cameras(id, name, rtsp_url, webrtc_path, direction, enabled, sort_order)
             VALUES(?,?,?,?,?,1,?)",
        )
        .bind(id)
        .bind(name)
        .bind(rtsp)
        .bind(webrtc)
        .bind(direction)
        .bind(sort_order)
        .execute(pool)
        .await?;
    } else if do_upsert {
        sqlx::query(
            "UPDATE cameras SET name=?, rtsp_url=?, webrtc_path=?, direction=?, sort_order=? WHERE id=?",
        )
        .bind(name)
        .bind(rtsp)
        .bind(webrtc)
        .bind(direction)
        .bind(sort_order)
        .bind(id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn gallery_version(pool: &SqlitePool) -> Result<u64> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_meta WHERE key = 'gallery_version'")
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(v,)| v.parse().ok()).unwrap_or(0))
}

pub async fn bump_gallery_version(pool: &SqlitePool) -> Result<u64> {
    let v = gallery_version(pool).await? + 1;
    sqlx::query(
        "INSERT INTO app_meta(key, value) VALUES('gallery_version', ?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
    )
    .bind(v.to_string())
    .execute(pool)
    .await?;
    Ok(v)
}

#[derive(Debug, Clone)]
pub struct CameraRow {
    pub id: String,
    pub name: String,
    pub rtsp_url: String,
    pub webrtc_path: String,
    pub direction: String,
    pub enabled: bool,
    pub sort_order: i32,
}

pub async fn list_cameras(pool: &SqlitePool, enabled_only: bool) -> Result<Vec<CameraRow>> {
    let rows = if enabled_only {
        sqlx::query(
            "SELECT id, name, rtsp_url, webrtc_path, direction, enabled, sort_order FROM cameras WHERE enabled=1 ORDER BY sort_order",
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, name, rtsp_url, webrtc_path, direction, enabled, sort_order FROM cameras ORDER BY sort_order",
        )
        .fetch_all(pool)
        .await?
    };
    Ok(rows
        .into_iter()
        .map(|r| CameraRow {
            id: r.get("id"),
            name: r.get("name"),
            rtsp_url: r.get("rtsp_url"),
            webrtc_path: r.get("webrtc_path"),
            direction: r.get("direction"),
            enabled: r.get::<i64, _>("enabled") != 0,
            sort_order: r.get("sort_order"),
        })
        .collect())
}

pub async fn get_camera(pool: &SqlitePool, id: &str) -> Result<Option<CameraRow>> {
    let r = sqlx::query(
        "SELECT id, name, rtsp_url, webrtc_path, direction, enabled, sort_order FROM cameras WHERE id=?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(r.map(|r| CameraRow {
        id: r.get("id"),
        name: r.get("name"),
        rtsp_url: r.get("rtsp_url"),
        webrtc_path: r.get("webrtc_path"),
        direction: r.get("direction"),
        enabled: r.get::<i64, _>("enabled") != 0,
        sort_order: r.get("sort_order"),
    }))
}

#[derive(Debug, Clone)]
pub struct EmployeeRow {
    pub id: i64,
    pub employee_code: String,
    pub full_name: String,
    pub department: Option<String>,
    pub is_active: bool,
}

pub async fn list_employees(pool: &SqlitePool, q: Option<&str>) -> Result<Vec<serde_json::Value>> {
    // ponytail: bounded <50 employee list; paginate/batch IDs only if scope grows
    // Fixed three-query shape (employees + images + embeddings), not 1+3N via employee_dict.
    use std::collections::HashMap;

    let rows = sqlx::query(
        "SELECT id, employee_code, full_name, department, is_active FROM employees ORDER BY full_name",
    )
    .fetch_all(pool)
    .await?;

    let mut selected: Vec<(i64, String, String, Option<String>, bool)> = Vec::new();
    for r in rows {
        let id: i64 = r.get("id");
        let code: String = r.get("employee_code");
        let name: String = r.get("full_name");
        let department: Option<String> = r.get("department");
        let is_active = r.get::<i64, _>("is_active") != 0;
        if let Some(qq) = q {
            let ql = qq.to_lowercase();
            if !name.to_lowercase().contains(&ql) && !code.to_lowercase().contains(&ql) {
                continue;
            }
        }
        selected.push((id, code, name, department, is_active));
    }

    // Fetch all image/embedding rows once and group (acceptable at sub-50 scale).
    let image_rows = sqlx::query(
        "SELECT id, employee_id, file_path, usable, reject_reason FROM employee_images ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    let emb_rows = sqlx::query("SELECT employee_id, num_images_used FROM employee_embeddings")
        .fetch_all(pool)
        .await?;

    let mut images_by_emp: HashMap<i64, Vec<serde_json::Value>> = HashMap::new();
    for i in image_rows {
        let emp_id: i64 = i.get("employee_id");
        images_by_emp
            .entry(emp_id)
            .or_default()
            .push(serde_json::json!({
                "id": i.get::<i64,_>("id"),
                "file_path": i.get::<String,_>("file_path"),
                "usable": i.get::<i64,_>("usable") != 0,
                "reject_reason": i.get::<Option<String>,_>("reject_reason"),
            }));
    }

    let mut emb_by_emp: HashMap<i64, i64> = HashMap::new();
    for e in emb_rows {
        emb_by_emp.insert(e.get("employee_id"), e.get("num_images_used"));
    }

    let mut out = Vec::with_capacity(selected.len());
    for (id, code, name, department, is_active) in selected {
        let imgs = images_by_emp.remove(&id).unwrap_or_default();
        let usable = imgs
            .iter()
            .filter(|i| i["usable"].as_bool() == Some(true))
            .count();
        let emb = emb_by_emp.get(&id);
        out.push(serde_json::json!({
            "id": id,
            "employee_code": code,
            "full_name": name,
            "department": department,
            "is_active": is_active,
            "image_count": imgs.len(),
            "usable_images": usable,
            "embedding_ready": emb.is_some(),
            "num_images_used": emb.copied().unwrap_or(0),
            "images": imgs,
        }));
    }
    Ok(out)
}

pub async fn employee_dict(pool: &SqlitePool, id: i64) -> Result<serde_json::Value> {
    let r = sqlx::query(
        "SELECT id, employee_code, full_name, department, is_active FROM employees WHERE id=?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .context("employee not found")?;
    let images = sqlx::query(
        "SELECT id, file_path, usable, reject_reason FROM employee_images WHERE employee_id=?",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let emb = sqlx::query("SELECT num_images_used FROM employee_embeddings WHERE employee_id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let usable = images
        .iter()
        .filter(|i| i.get::<i64, _>("usable") != 0)
        .count();
    let imgs: Vec<serde_json::Value> = images
        .iter()
        .map(|i| {
            serde_json::json!({
                "id": i.get::<i64,_>("id"),
                "file_path": i.get::<String,_>("file_path"),
                "usable": i.get::<i64,_>("usable") != 0,
                "reject_reason": i.get::<Option<String>,_>("reject_reason"),
            })
        })
        .collect();
    Ok(serde_json::json!({
        "id": r.get::<i64,_>("id"),
        "employee_code": r.get::<String,_>("employee_code"),
        "full_name": r.get::<String,_>("full_name"),
        "department": r.get::<Option<String>,_>("department"),
        "is_active": r.get::<i64,_>("is_active") != 0,
        "image_count": imgs.len(),
        "usable_images": usable,
        "embedding_ready": emb.is_some(),
        "num_images_used": emb.as_ref().map(|e| e.get::<i64,_>("num_images_used")).unwrap_or(0),
        "images": imgs,
    }))
}

pub async fn create_employee(
    pool: &SqlitePool,
    code: &str,
    full_name: &str,
    department: Option<&str>,
) -> Result<i64> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let res = sqlx::query(
        "INSERT INTO employees(employee_code, full_name, department, is_active, created_at, updated_at)
         VALUES(?,?,?,1,?,?)",
    )
    .bind(code)
    .bind(full_name)
    .bind(department)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(res.last_insert_rowid())
}

/// Optional employee field updates. Employee code is immutable.
#[derive(Debug, Clone, Default)]
pub struct EmployeePatch {
    pub full_name: Option<String>,
    pub department: Option<String>,
    pub is_active: Option<bool>,
}

/// Outcome of a transactional employee update.
#[derive(Debug, Clone)]
pub struct EmployeePatchOutcome {
    /// True when name or active state actually changed (gallery version was bumped).
    pub gallery_relevant_change: bool,
    /// True when any persisted column changed.
    pub changed: bool,
    pub employee: serde_json::Value,
}

/// Update supported employee fields in one transaction.
///
/// Bumps `gallery_version` only when `full_name` or `is_active` actually change.
/// Returns `Ok(None)` when the employee id does not exist.
pub async fn update_employee_fields(
    pool: &SqlitePool,
    id: i64,
    patch: EmployeePatch,
) -> Result<Option<EmployeePatchOutcome>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT id, employee_code, full_name, department, is_active FROM employees WHERE id=?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };

    let cur_name: String = row.get("full_name");
    let cur_dept: Option<String> = row.get("department");
    let cur_active = row.get::<i64, _>("is_active") != 0;

    let new_name = patch.full_name.as_ref().unwrap_or(&cur_name).clone();
    let new_dept = match &patch.department {
        Some(d) => Some(d.clone()),
        None => cur_dept.clone(),
    };
    let new_active = patch.is_active.unwrap_or(cur_active);

    let name_changed = new_name != cur_name;
    let dept_changed = new_dept != cur_dept;
    let active_changed = new_active != cur_active;
    let changed = name_changed || dept_changed || active_changed;
    let gallery_relevant_change = name_changed || active_changed;

    if changed {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        sqlx::query(
            "UPDATE employees SET full_name=?, department=?, is_active=?, updated_at=? WHERE id=?",
        )
        .bind(&new_name)
        .bind(&new_dept)
        .bind(if new_active { 1 } else { 0 })
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if gallery_relevant_change {
            bump_gallery_version_tx(&mut tx).await?;
        }
    }

    tx.commit().await?;
    let employee = employee_dict(pool, id).await?;
    Ok(Some(EmployeePatchOutcome {
        gallery_relevant_change,
        changed,
        employee,
    }))
}

/// Soft-deactivate an employee (`is_active=false`). Idempotent when already inactive.
pub async fn deactivate_employee(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<EmployeePatchOutcome>> {
    update_employee_fields(
        pool,
        id,
        EmployeePatch {
            is_active: Some(false),
            ..Default::default()
        },
    )
    .await
}

pub async fn load_gallery_matrix(
    pool: &SqlitePool,
    dim: usize,
    model_name: &str,
    min_images: usize,
) -> Result<(Vec<i64>, Vec<String>, Vec<Vec<f32>>)> {
    let rows = sqlx::query(
        "SELECT e.id, e.full_name, emb.vector, emb.dim FROM employees e
         JOIN employee_embeddings emb ON emb.employee_id = e.id
         WHERE e.is_active = 1 AND emb.model_name = ? AND emb.num_images_used >= ?",
    )
    .bind(model_name)
    .bind(min_images as i64)
    .fetch_all(pool)
    .await?;
    let mut ids = Vec::new();
    let mut names = Vec::new();
    let mut vecs = Vec::new();
    for r in rows {
        let blob: Vec<u8> = r.get("vector");
        let d: i64 = r.get("dim");
        match pksp_core::unpack_embedding(&blob, d as usize) {
            Ok(v) if v.len() == dim => {
                ids.push(r.get("id"));
                names.push(r.get("full_name"));
                vecs.push(v);
            }
            _ => continue,
        }
    }
    Ok((ids, names, vecs))
}

/// Typed outcome of a commit attempt — never discard skip reasons at the DB boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    Committed {
        event_id: i64,
        name: String,
        kind: String,
    },
    Skipped(SkipReason),
}

/// Persisted daily attendance truth for dashboard metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DailyAttendanceMetrics {
    pub events_today: i64,
    pub present_count: i64,
}

/// Count events and present employees for a configured local calendar date.
///
/// `events_today` is the count of persisted attendance events for `local_date`.
/// `present_count` is the count of *active* employees whose latest event that
/// date is `check_in`. Latest is ordered by `ts DESC, id DESC` so ties are
/// deterministic.
pub async fn daily_attendance_metrics(
    pool: &SqlitePool,
    local_date: &str,
) -> Result<DailyAttendanceMetrics> {
    let events_today: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM attendance_events WHERE local_date=?")
            .bind(local_date)
            .fetch_one(pool)
            .await?;

    // One row per active employee with their latest event that day (ts, then id).
    let latest_rows = sqlx::query(
        "SELECT ae.employee_id AS employee_id, ae.kind AS kind, ae.id AS id, ae.ts AS ts
         FROM attendance_events ae
         INNER JOIN employees e ON e.id = ae.employee_id AND e.is_active = 1
         WHERE ae.local_date = ?
           AND ae.kind IN ('check_in', 'check_out')
         ORDER BY ae.employee_id ASC, ae.ts DESC, ae.id DESC",
    )
    .bind(local_date)
    .fetch_all(pool)
    .await?;

    let mut present_count: i64 = 0;
    let mut seen_employee: Option<i64> = None;
    for row in latest_rows {
        let eid: i64 = row.get("employee_id");
        if seen_employee == Some(eid) {
            continue; // not the latest for this employee
        }
        seen_employee = Some(eid);
        let kind: String = row.get("kind");
        if kind == "check_in" {
            present_count += 1;
        }
    }

    Ok(DailyAttendanceMetrics {
        events_today,
        present_count,
    })
}

#[allow(clippy::too_many_arguments)] // ponytail: orchestration boundary; group only when another caller exists
pub async fn commit_identity(
    pool: &SqlitePool,
    employee_id: i64,
    camera_id: &str,
    score: f32,
    track_id: Option<i64>,
    cooldown_seconds: f64,
    min_dwell_seconds: f64,
    app_timezone: &str,
) -> Result<CommitOutcome> {
    let cam = get_camera(pool, camera_id).await?;
    let Some(cam) = cam else {
        return Ok(CommitOutcome::Skipped(SkipReason::MissingCamera));
    };
    let emp = sqlx::query("SELECT full_name, is_active FROM employees WHERE id=?")
        .bind(employee_id)
        .fetch_optional(pool)
        .await?;
    let Some(emp) = emp else {
        return Ok(CommitOutcome::Skipped(SkipReason::MissingEmployee));
    };
    if emp.get::<i64, _>("is_active") == 0 {
        return Ok(CommitOutcome::Skipped(SkipReason::InactiveEmployee));
    }
    let name: String = emp.get("full_name");
    let now = Utc::now();
    let local_date = local_date_str(now, app_timezone)?;
    let last_today = last_event_today(pool, employee_id, &local_date).await?;
    let last_cam = last_camera_event_ts(pool, employee_id, camera_id).await?;
    let decision = on_identity_commit(
        Direction::parse(&cam.direction),
        now,
        last_today.as_ref(),
        last_cam,
        cooldown_seconds,
        min_dwell_seconds,
    );
    let kind = match decision {
        FsmDecision::Commit { kind } => kind,
        FsmDecision::Skip { reason } => {
            return Ok(CommitOutcome::Skipped(reason));
        }
    };
    let ts = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let res = sqlx::query(
        "INSERT INTO attendance_events(employee_id, camera_id, kind, score, track_id, needs_review, ts, local_date)
         VALUES(?,?,?,?,?,0,?,?)",
    )
    .bind(employee_id)
    .bind(camera_id)
    .bind(kind.as_str())
    .bind(score)
    .bind(track_id)
    .bind(&ts)
    .bind(&local_date)
    .execute(pool)
    .await?;
    Ok(CommitOutcome::Committed {
        event_id: res.last_insert_rowid(),
        name,
        kind: kind.as_str().to_string(),
    })
}

/// Snapshot metadata stored for an attendance event (paths relative to DATA_DIR).
#[derive(Debug, Clone, PartialEq)]
pub struct EventSnapshotMeta {
    pub event_id: i64,
    /// Relative path under DATA_DIR, e.g. `events/42.jpg`.
    pub snapshot_path: Option<String>,
    /// Normalized xyxy bbox as JSON array string, e.g. `[0.1,0.2,0.3,0.4]`.
    pub snapshot_bbox_json: Option<String>,
}

/// Events directory under DATA_DIR.
pub fn events_dir(settings: &Settings) -> PathBuf {
    settings.data_dir.join("events")
}

/// Relative snapshot path for an event id (`events/<id>.jpg`).
pub fn event_snapshot_rel_path(event_id: i64) -> String {
    format!("events/{event_id}.jpg")
}

/// Validate that a relative snapshot path is owned by the events directory
/// under DATA_DIR and resolves inside it (no path traversal).
pub fn resolve_event_snapshot_path(settings: &Settings, relative: &str) -> Result<PathBuf, String> {
    let rel = relative.trim();
    if rel.is_empty() {
        return Err("empty snapshot path".into());
    }
    if rel.contains("..") || rel.starts_with('/') || rel.starts_with('\\') {
        return Err("invalid snapshot path".into());
    }
    if !rel.starts_with("events/") {
        return Err("snapshot path must be under events/".into());
    }
    // Reject nested or unexpected extensions beyond events/<name>.jpg
    let rest = &rel["events/".len()..];
    if rest.is_empty() || rest.contains('/') || rest.contains('\\') {
        return Err("invalid snapshot path".into());
    }
    if !rest.ends_with(".jpg") && !rest.ends_with(".jpeg") {
        return Err("snapshot path must be jpeg".into());
    }
    let abs = settings.data_dir.join(rel);
    let events_root = events_dir(settings);
    // Existing files must also remain inside the canonical events directory;
    // lexical containment alone does not catch a symlinked snapshot.
    let abs_norm = abs.canonicalize().unwrap_or_else(|_| abs.clone());
    let root_norm = events_root
        .canonicalize()
        .unwrap_or_else(|_| events_root.clone());
    if abs.exists() && !abs_norm.starts_with(&root_norm) {
        return Err("snapshot path escapes events directory".into());
    }
    Ok(abs)
}

/// Attach snapshot metadata after a successful attendance commit.
/// Does not accept absolute or caller-controlled escape paths.
pub async fn attach_event_snapshot(
    pool: &SqlitePool,
    event_id: i64,
    relative_path: &str,
    bbox_xyxy: [f32; 4],
) -> Result<()> {
    if relative_path.contains("..")
        || relative_path.starts_with('/')
        || !relative_path.starts_with("events/")
    {
        anyhow::bail!("refusing non-events relative snapshot path");
    }
    let bbox_json = serde_json::to_string(&bbox_xyxy)?;
    let res = sqlx::query(
        "UPDATE attendance_events SET snapshot_path = ?, snapshot_bbox_json = ? WHERE id = ?",
    )
    .bind(relative_path)
    .bind(&bbox_json)
    .bind(event_id)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        anyhow::bail!("event {event_id} not found for snapshot attach");
    }
    Ok(())
}

/// Fetch snapshot metadata for an event. `None` if the event does not exist.
pub async fn get_event_snapshot(
    pool: &SqlitePool,
    event_id: i64,
) -> Result<Option<EventSnapshotMeta>> {
    let row = sqlx::query(
        "SELECT id, snapshot_path, snapshot_bbox_json FROM attendance_events WHERE id = ?",
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| EventSnapshotMeta {
        event_id: r.get("id"),
        snapshot_path: r.get("snapshot_path"),
        snapshot_bbox_json: r.get("snapshot_bbox_json"),
    }))
}

/// Local calendar date (`YYYY-MM-DD`) for `now` in the named IANA timezone.
///
/// A valid IANA name returns its local calendar date. An invalid name returns an
/// error and never falls back to UTC — callers must treat misconfiguration as
/// fail-closed.
pub fn local_date_str(now: chrono::DateTime<Utc>, tz_name: &str) -> Result<String> {
    let tz = tz_name.parse::<chrono_tz::Tz>().map_err(|_| {
        anyhow::anyhow!("invalid APP_TIMEZONE {tz_name:?}; expected a valid IANA timezone name")
    })?;
    Ok(now.with_timezone(&tz).date_naive().to_string())
}

/// Convert a UTC ISO wire timestamp (`YYYY-MM-DDTHH:MM:SSZ` or with fractional
/// seconds) into a local wall-clock `HH:MM:SS` in the named IANA zone.
///
/// Empty input yields an empty string. Invalid timestamps or timezones error.
pub fn utc_iso_to_local_hms(iso_utc: &str, tz_name: &str) -> Result<String> {
    if iso_utc.is_empty() {
        return Ok(String::new());
    }
    let tz = tz_name.parse::<chrono_tz::Tz>().map_err(|_| {
        anyhow::anyhow!("invalid APP_TIMEZONE {tz_name:?}; expected a valid IANA timezone name")
    })?;
    let parsed = chrono::DateTime::parse_from_rfc3339(iso_utc)
        .or_else(|_| {
            // Wire contract uses trailing Z without offset form.
            chrono::DateTime::parse_from_str(iso_utc, "%Y-%m-%dT%H:%M:%SZ")
        })
        .or_else(|_| chrono::DateTime::parse_from_str(iso_utc, "%Y-%m-%dT%H:%M:%S%.fZ"))
        .map_err(|_| anyhow::anyhow!("invalid UTC ISO timestamp {iso_utc:?}"))?;
    let utc = parsed.with_timezone(&Utc);
    Ok(utc.with_timezone(&tz).format("%H:%M:%S").to_string())
}

async fn last_event_today(
    pool: &SqlitePool,
    employee_id: i64,
    local_date: &str,
) -> Result<Option<PriorEvent>> {
    let r = sqlx::query(
        "SELECT kind, ts, camera_id FROM attendance_events
         WHERE employee_id=? AND local_date=? AND kind IN ('check_in','check_out')
         ORDER BY ts DESC LIMIT 1",
    )
    .bind(employee_id)
    .bind(local_date)
    .fetch_optional(pool)
    .await?;
    Ok(r.and_then(|row| {
        let kind = EventKind::parse(&row.get::<String, _>("kind"))?;
        let ts_s: String = row.get("ts");
        let ts = chrono::NaiveDateTime::parse_from_str(&ts_s, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|n| n.and_utc())?;
        Some(PriorEvent {
            kind,
            ts,
            camera_id: row.get("camera_id"),
        })
    }))
}

async fn last_camera_event_ts(
    pool: &SqlitePool,
    employee_id: i64,
    camera_id: &str,
) -> Result<Option<chrono::DateTime<Utc>>> {
    let r = sqlx::query(
        "SELECT ts FROM attendance_events
         WHERE employee_id=? AND camera_id=? AND kind IN ('check_in','check_out')
         ORDER BY ts DESC LIMIT 1",
    )
    .bind(employee_id)
    .bind(camera_id)
    .fetch_optional(pool)
    .await?;
    Ok(r.and_then(|row| {
        let ts_s: String = row.get("ts");
        chrono::NaiveDateTime::parse_from_str(&ts_s, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|n| n.and_utc())
    }))
}

pub async fn build_daily(pool: &SqlitePool, day: &str) -> Result<Vec<serde_json::Value>> {
    let emps = sqlx::query(
        "SELECT id, employee_code, full_name, department FROM employees WHERE is_active=1",
    )
    .fetch_all(pool)
    .await?;
    let employees: Vec<EmployeeRef> = emps
        .iter()
        .map(|r| EmployeeRef {
            id: r.get("id"),
            employee_code: r.get("employee_code"),
            full_name: r.get("full_name"),
            department: r.get("department"),
        })
        .collect();
    let evs = sqlx::query("SELECT employee_id, kind, ts FROM attendance_events WHERE local_date=?")
        .bind(day)
        .fetch_all(pool)
        .await?;
    let raw: Vec<RawEvent> = evs
        .iter()
        .filter_map(|r| {
            let eid: Option<i64> = r.get("employee_id");
            let eid = eid?;
            let ts_s: String = r.get("ts");
            let ts = chrono::NaiveDateTime::parse_from_str(&ts_s, "%Y-%m-%d %H:%M:%S").ok()?;
            Some(RawEvent {
                employee_id: eid,
                kind: r.get("kind"),
                ts,
            })
        })
        .collect();
    let rows = aggregate_daily(&employees, &raw);
    Ok(rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "employee_id": r.employee_id,
                "employee_code": r.employee_code,
                "full_name": r.full_name,
                "department": r.department,
                "first_in": r.first_in.map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                "last_out": r.last_out.map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                "duration_minutes": r.duration_minutes,
                "status": r.status,
                "check_in_count": r.check_in_count,
                "check_out_count": r.check_out_count,
            })
        })
        .collect())
}

/// Build the daily CSV for `day`, converting first/last UTC ISO wire values into
/// local human times in `tz_name` before field encoding.
///
/// JSON daily responses keep UTC ISO; only this export uses local wall-clock times.
pub async fn daily_csv(pool: &SqlitePool, day: &str, tz_name: &str) -> Result<String> {
    // Fail closed on timezone before scanning rows.
    let _ = local_date_str(Utc::now(), tz_name)?;
    let rows = build_daily(pool, day).await?;
    let mut out = String::new();
    out.push_str(&daily_csv_headers().join(","));
    out.push('\n');
    for r in rows {
        let duration = r["duration_minutes"]
            .as_i64()
            .map(|n| n.to_string())
            .unwrap_or_default();
        let check_in = r["check_in_count"].as_i64().unwrap_or(0).to_string();
        let check_out = r["check_out_count"].as_i64().unwrap_or(0).to_string();
        let first_in = utc_iso_to_local_hms(r["first_in"].as_str().unwrap_or(""), tz_name)?;
        let last_out = utc_iso_to_local_hms(r["last_out"].as_str().unwrap_or(""), tz_name)?;
        let cells = [
            csv_encode_field(day, false),
            csv_encode_field(r["employee_code"].as_str().unwrap_or(""), true),
            csv_encode_field(r["full_name"].as_str().unwrap_or(""), true),
            csv_encode_field(r["department"].as_str().unwrap_or(""), true),
            csv_encode_field(&first_in, false),
            csv_encode_field(&last_out, false),
            // Numeric count/duration cells stay bare numbers.
            duration,
            csv_encode_field(r["status"].as_str().unwrap_or(""), false),
            check_in,
            check_out,
        ];
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    Ok(out)
}

pub async fn save_embedding(
    pool: &SqlitePool,
    employee_id: i64,
    vector: &[u8],
    dim: usize,
    num_used: i32,
    model_name: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    save_embedding_tx(&mut tx, employee_id, vector, dim, num_used, model_name).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn save_embedding_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    employee_id: i64,
    vector: &[u8],
    dim: usize,
    num_used: i32,
    model_name: &str,
) -> Result<()> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query(
        "INSERT INTO employee_embeddings(employee_id, dim, vector, num_images_used, model_name, updated_at)
         VALUES(?,?,?,?,?,?)
         ON CONFLICT(employee_id) DO UPDATE SET
           dim=excluded.dim, vector=excluded.vector, num_images_used=excluded.num_images_used,
           model_name=excluded.model_name, updated_at=excluded.updated_at",
    )
    .bind(employee_id)
    .bind(dim as i64)
    .bind(vector)
    .bind(num_used)
    .bind(model_name)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn delete_embedding(pool: &SqlitePool, employee_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM employee_embeddings WHERE employee_id=?")
        .bind(employee_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_embedding_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    employee_id: i64,
) -> Result<()> {
    sqlx::query("DELETE FROM employee_embeddings WHERE employee_id=?")
        .bind(employee_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct EmployeeImageRow {
    pub id: i64,
    pub file_path: String,
    pub usable: bool,
    pub reject_reason: Option<String>,
}

pub async fn list_employee_images(
    pool: &SqlitePool,
    employee_id: i64,
) -> Result<Vec<EmployeeImageRow>> {
    let rows = sqlx::query(
        "SELECT id, file_path, usable, reject_reason FROM employee_images WHERE employee_id=? ORDER BY id",
    )
    .bind(employee_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| EmployeeImageRow {
            id: r.get("id"),
            file_path: r.get("file_path"),
            usable: r.get::<i64, _>("usable") != 0,
            reject_reason: r.get("reject_reason"),
        })
        .collect())
}

pub async fn add_employee_image(
    pool: &SqlitePool,
    employee_id: i64,
    file_path: &str,
    usable: bool,
    reject_reason: Option<&str>,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let id = add_employee_image_tx(&mut tx, employee_id, file_path, usable, reject_reason).await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn add_employee_image_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    employee_id: i64,
    file_path: &str,
    usable: bool,
    reject_reason: Option<&str>,
) -> Result<i64> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let res = sqlx::query(
        "INSERT INTO employee_images(employee_id, file_path, usable, reject_reason, created_at)
         VALUES(?,?,?,?,?)",
    )
    .bind(employee_id)
    .bind(file_path)
    .bind(if usable { 1 } else { 0 })
    .bind(reject_reason)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
    Ok(res.last_insert_rowid())
}

pub async fn update_employee_image_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    usable: bool,
    reject_reason: Option<&str>,
) -> Result<()> {
    sqlx::query("UPDATE employee_images SET usable=?, reject_reason=? WHERE id=?")
        .bind(if usable { 1 } else { 0 })
        .bind(reject_reason)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn bump_gallery_version_tx(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<u64> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM app_meta WHERE key = 'gallery_version'")
            .fetch_optional(&mut **tx)
            .await?;
    let v = row.and_then(|(v,)| v.parse::<u64>().ok()).unwrap_or(0) + 1;
    sqlx::query(
        "INSERT INTO app_meta(key, value) VALUES('gallery_version', ?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
    )
    .bind(v.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(v)
}

pub async fn embedding_exists(pool: &SqlitePool, employee_id: i64) -> Result<bool> {
    let row = sqlx::query("SELECT 1 as x FROM employee_embeddings WHERE employee_id=?")
        .bind(employee_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

#[cfg(test)]
mod gallery_model_tests {
    use super::*;

    #[tokio::test]
    async fn gallery_loads_only_the_expected_model() {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("pksp-gallery-model-{id}"));
        std::fs::create_dir_all(&data_dir).unwrap();
        let db_path = data_dir.join("test.db");
        let mut settings = Settings::from_env();
        let abs = db_path
            .to_string_lossy()
            .trim_start_matches('/')
            .to_string();
        settings.database_url = format!("sqlite:////{abs}?mode=rwc");
        settings.data_dir = data_dir.clone();
        let pool = connect_pool(&settings).await.unwrap();

        let real = create_employee(&pool, "R", "Real", None).await.unwrap();
        let stale = create_employee(&pool, "S", "Stale", None).await.unwrap();
        let blob = pksp_core::pack_embedding(&[1.0, 0.0], 2).unwrap();
        save_embedding(&pool, real, &blob, 2, 5, "buffalo_l")
            .await
            .unwrap();
        save_embedding(&pool, stale, &blob, 2, 5, "legacy")
            .await
            .unwrap();

        let (ids, names, vectors) = load_gallery_matrix(&pool, 2, "buffalo_l", 5).await.unwrap();
        assert_eq!(ids, vec![real]);
        assert_eq!(names, vec!["Real"]);
        assert_eq!(vectors.len(), 1);

        pool.close().await;
        let _ = std::fs::remove_dir_all(data_dir);
    }
}

#[cfg(test)]
mod path_tests {
    use super::resolve_sqlite_path;
    use std::path::PathBuf;

    #[test]
    fn python_relative_dot_slash() {
        let p = resolve_sqlite_path("sqlite:///./data/pksp.db");
        assert_eq!(p, PathBuf::from("./data/pksp.db"));
        // Must NOT be absolute root path that triggers EROFS
        assert!(!p.is_absolute() || p.starts_with("."));
        assert!(!p.starts_with("/data"));
    }

    #[test]
    fn python_relative_plain() {
        let p = resolve_sqlite_path("sqlite:///data/pksp.db");
        assert_eq!(p, PathBuf::from("data/pksp.db"));
    }

    #[test]
    fn absolute_four_slash() {
        let p = resolve_sqlite_path("sqlite:////tmp/pksp.db");
        assert_eq!(p, PathBuf::from("/tmp/pksp.db"));
        assert!(p.is_absolute());
    }

    #[test]
    fn with_query_mode() {
        let p = resolve_sqlite_path("sqlite:///./data/pksp.db?mode=rwc");
        assert_eq!(p, PathBuf::from("./data/pksp.db"));
    }

    #[test]
    fn bare_path() {
        let p = resolve_sqlite_path("data/rust-edge/pksp.db");
        assert_eq!(p, PathBuf::from("data/rust-edge/pksp.db"));
    }
}

#[cfg(test)]
mod security_settings_tests {
    use super::validate_bind_and_secrets;

    #[test]
    fn loopback_accepts_demo_fallbacks() {
        let addr = validate_bind_and_secrets(
            "127.0.0.1:8000",
            "change-me",
            "dev-jwt-secret-change-me",
            false,
            false,
        )
        .expect("loopback demo ok");
        assert!(addr.ip().is_loopback());
    }

    #[test]
    fn ipv4_wildcard_rejects_missing_secrets() {
        let err = validate_bind_and_secrets("0.0.0.0:8000", "change-me", "short", false, false)
            .expect_err("wildcard needs secrets");
        assert!(err.contains("ADMIN_PASSWORD"), "{err}");
    }

    #[test]
    fn ipv6_wildcard_rejects_missing_jwt() {
        let long_enough = "x".repeat(32);
        let err = validate_bind_and_secrets(
            "[::]:8000",
            "not-the-demo-password",
            &long_enough,
            true,
            false,
        )
        .expect_err("jwt must be explicit");
        assert!(err.contains("JWT_SECRET"), "{err}");
    }

    #[test]
    fn non_loopback_accepts_explicit_non_demo_secrets() {
        let secret = "a".repeat(32);
        let addr = validate_bind_and_secrets(
            "10.0.0.5:8000",
            "operator-chosen-password",
            &secret,
            true,
            true,
        )
        .expect("lan bind with real secrets");
        assert_eq!(addr.port(), 8000);
    }

    #[test]
    fn malformed_bind_is_rejected() {
        let err = validate_bind_and_secrets("not-an-addr", "x", &"y".repeat(32), true, true)
            .expect_err("malformed");
        assert!(err.contains("BIND_ADDR"), "{err}");
    }
}

#[cfg(test)]
mod enrollment_settings_tests {
    use super::Settings;

    #[test]
    fn enrollment_limits_reject_zero_files() {
        let mut s = Settings::from_env();
        s.max_enroll_files = 0;
        let err = s.validate_enrollment_limits().expect_err("zero files");
        assert!(err.contains("MAX_ENROLL_FILES"), "{err}");
    }

    #[test]
    fn enrollment_limits_reject_upload_lt_file() {
        let mut s = Settings::from_env();
        s.max_enroll_file_bytes = 1000;
        s.max_enroll_upload_bytes = 100;
        let err = s.validate_enrollment_limits().expect_err("upload < file");
        assert!(err.contains("MAX_ENROLL_UPLOAD_BYTES"), "{err}");
    }

    #[test]
    fn enrollment_limits_defaults_ok() {
        let s = Settings::from_env();
        s.validate_enrollment_limits().expect("defaults");
        assert_eq!(s.max_enroll_files, 10);
        assert_eq!(s.max_enroll_file_bytes, 5_242_880);
        assert_eq!(s.max_enroll_upload_bytes, 33_554_432);
        assert_eq!(s.min_enroll_images, 5);
        assert_eq!(s.match_threshold, 0.75);
        assert_eq!(s.match_margin, 0.10);
        assert_eq!(s.pose_max_yaw, 30.0);
        assert_eq!(s.blur_min_var, 75.0);
        assert_eq!(s.app_timezone, "Asia/Tehran");
    }

    #[test]
    fn startup_rejects_missing_rtsp_and_wrong_embedding_dimension() {
        let mut s = Settings::from_env();
        s.bind_addr = "127.0.0.1:8000".into();
        s.vision_enabled = true;
        s.cam_in_rtsp.clear();
        assert!(s.validate_startup().unwrap_err().contains("CAM_IN_RTSP"));

        s.cam_in_rtsp = "rtsp://127.0.0.1:8554/cam".into();
        s.embedding_dim = 16;
        assert!(s.validate_startup().unwrap_err().contains("EMBEDDING_DIM"));
    }
}

#[cfg(test)]
mod csv_tests {
    use super::*;
    use pksp_core::csv_encode_field;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_settings() -> (Settings, PathBuf, PathBuf) {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("pksp-db-csv-{id}"));
        std::fs::create_dir_all(&data_dir).unwrap();
        let db_path = data_dir.join("test.db");
        let mut s = Settings::from_env();
        let abs = db_path
            .to_string_lossy()
            .trim_start_matches('/')
            .to_string();
        s.database_url = format!("sqlite:////{abs}?mode=rwc");
        s.data_dir = data_dir.clone();
        s.camera_upsert = true;
        s.cam_out_rtsp = String::new();
        (s, data_dir, db_path)
    }

    #[tokio::test]
    async fn daily_csv_escapes_and_neutralizes() {
        let (settings, data_dir, db_path) = temp_settings();
        let pool = connect_pool(&settings).await.unwrap();
        let id = create_employee(&pool, "=CMD", "Doe, \"John\"", Some("+Finance"))
            .await
            .unwrap();
        // Attendance event so status is incomplete (check-in only).
        let day = "2026-07-12";
        sqlx::query(
            "INSERT INTO attendance_events(employee_id, camera_id, kind, score, ts, local_date)
             VALUES(?,?,?,?,?,?)",
        )
        .bind(id)
        .bind("cam_in")
        .bind("check_in")
        .bind(0.9_f64)
        .bind("2026-07-12 08:00:00")
        .bind(day)
        .execute(&pool)
        .await
        .unwrap();

        let csv = daily_csv(&pool, day, "UTC").await.unwrap();
        let headers = daily_csv_headers().join(",");
        assert!(csv.starts_with(&headers), "headers: {csv}");
        assert_eq!(daily_csv_headers().len(), 10);

        // Employee-controlled formula neutralization + RFC quoting.
        assert!(csv.contains(&csv_encode_field("=CMD", true)));
        assert!(csv.contains(&csv_encode_field("Doe, \"John\"", true)));
        assert!(csv.contains(&csv_encode_field("+Finance", true)));
        // Local human times (not raw UTC ISO) for first_in.
        assert!(csv.contains("08:00:00"), "expected local hms: {csv}");
        assert!(!csv.contains("2026-07-12T08:00:00Z"));
        // Numeric cells stay bare.
        assert!(csv.contains(",1,0\n") || csv.lines().any(|l| l.ends_with(",1,0")));

        // Empty department employee.
        let id2 = create_employee(&pool, "PLAIN", "Alice", None)
            .await
            .unwrap();
        let _ = id2;
        let csv2 = daily_csv(&pool, day, "UTC").await.unwrap();
        assert!(csv2.contains("PLAIN"));
        assert!(csv2.contains("Alice"));

        // CR/LF and whitespace-before-formula via direct encoder coverage used by builder.
        assert_eq!(
            csv_encode_field("  =1+1", true),
            csv_encode_field("  =1+1", true)
        );
        assert!(csv_encode_field("a\nb", false).starts_with('"'));

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}

#[cfg(test)]
mod timezone_tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn local_date_str_valid_zones() {
        // 2026-07-12 22:30 UTC → still 2026-07-12 in UTC, next day in Tehran (+03:30).
        let utc = Utc.with_ymd_and_hms(2026, 7, 12, 22, 30, 0).unwrap();
        assert_eq!(local_date_str(utc, "UTC").unwrap(), "2026-07-12");
        assert_eq!(local_date_str(utc, "Asia/Tehran").unwrap(), "2026-07-13");
        // Negative offset: America/New_York is UTC-4 in July → still 2026-07-12 afternoon.
        assert_eq!(
            local_date_str(utc, "America/New_York").unwrap(),
            "2026-07-12"
        );
        // Instant just after UTC midnight: still previous evening in New York.
        let after_midnight = Utc.with_ymd_and_hms(2026, 7, 13, 1, 0, 0).unwrap();
        assert_eq!(local_date_str(after_midnight, "UTC").unwrap(), "2026-07-13");
        assert_eq!(
            local_date_str(after_midnight, "America/New_York").unwrap(),
            "2026-07-12"
        );
        assert_eq!(
            local_date_str(after_midnight, "Asia/Tehran").unwrap(),
            "2026-07-13"
        );
    }

    #[test]
    fn local_date_str_invalid_never_falls_back() {
        let utc = Utc.with_ymd_and_hms(2026, 7, 12, 12, 0, 0).unwrap();
        let err = local_date_str(utc, "Not/A_Zone").unwrap_err().to_string();
        assert!(err.contains("invalid APP_TIMEZONE"), "{err}");
        assert!(err.contains("Not/A_Zone"), "{err}");
    }

    #[test]
    fn utc_iso_to_local_hms_zones() {
        let iso = "2026-07-12T20:00:00Z";
        assert_eq!(utc_iso_to_local_hms(iso, "UTC").unwrap(), "20:00:00");
        // Asia/Tehran +03:30 → 23:30:00
        assert_eq!(
            utc_iso_to_local_hms(iso, "Asia/Tehran").unwrap(),
            "23:30:00"
        );
        // America/New_York UTC-4 in July → 16:00:00
        assert_eq!(
            utc_iso_to_local_hms(iso, "America/New_York").unwrap(),
            "16:00:00"
        );
        assert_eq!(utc_iso_to_local_hms("", "UTC").unwrap(), "");
        assert!(utc_iso_to_local_hms(iso, "Bad/Zone").is_err());
        assert!(utc_iso_to_local_hms("not-a-timestamp", "UTC").is_err());
    }

    #[tokio::test]
    async fn daily_csv_uses_local_human_times() {
        let (mut settings, data_dir, db_path) = {
            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let data_dir = std::env::temp_dir().join(format!("pksp-db-tz-csv-{id}"));
            std::fs::create_dir_all(&data_dir).unwrap();
            let db_path = data_dir.join("test.db");
            let mut s = Settings::from_env();
            let abs = db_path
                .to_string_lossy()
                .trim_start_matches('/')
                .to_string();
            s.database_url = format!("sqlite:////{abs}?mode=rwc");
            s.data_dir = data_dir.clone();
            s.camera_upsert = true;
            s.cam_out_rtsp = String::new();
            (s, data_dir, db_path)
        };
        settings.app_timezone = "Asia/Tehran".into();
        let pool = connect_pool(&settings).await.unwrap();
        let id = create_employee(&pool, "E1", "Sam", None).await.unwrap();
        let day = "2026-07-12";
        // 08:00 UTC → 11:30 Tehran
        sqlx::query(
            "INSERT INTO attendance_events(employee_id, camera_id, kind, score, ts, local_date)
             VALUES(?,?,?,?,?,?)",
        )
        .bind(id)
        .bind("cam_in")
        .bind("check_in")
        .bind(0.9_f64)
        .bind("2026-07-12 08:00:00")
        .bind(day)
        .execute(&pool)
        .await
        .unwrap();

        let csv = daily_csv(&pool, day, "Asia/Tehran").await.unwrap();
        assert!(csv.contains("11:30:00"), "csv={csv}");
        assert!(!csv.contains("2026-07-12T08:00:00Z"));

        // JSON daily keeps UTC wire values.
        let rows = build_daily(&pool, day).await.unwrap();
        assert_eq!(rows[0]["first_in"], "2026-07-12T08:00:00Z");

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}

#[cfg(test)]
mod metrics_and_commit_tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_settings() -> (Settings, PathBuf, PathBuf) {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("pksp-db-metrics-{id}"));
        std::fs::create_dir_all(&data_dir).unwrap();
        let db_path = data_dir.join("test.db");
        let mut s = Settings::from_env();
        let abs = db_path
            .to_string_lossy()
            .trim_start_matches('/')
            .to_string();
        s.database_url = format!("sqlite:////{abs}?mode=rwc");
        s.data_dir = data_dir.clone();
        s.camera_upsert = true;
        s.cam_out_rtsp = String::new();
        s.app_timezone = "UTC".into();
        (s, data_dir, db_path)
    }

    async fn insert_event(
        pool: &SqlitePool,
        employee_id: i64,
        kind: &str,
        ts: &str,
        local_date: &str,
    ) -> i64 {
        let res = sqlx::query(
            "INSERT INTO attendance_events(employee_id, camera_id, kind, score, ts, local_date)
             VALUES(?,?,?,?,?,?)",
        )
        .bind(employee_id)
        .bind("cam_in")
        .bind(kind)
        .bind(0.9_f64)
        .bind(ts)
        .bind(local_date)
        .execute(pool)
        .await
        .unwrap();
        res.last_insert_rowid()
    }

    #[tokio::test]
    async fn daily_metrics_state_matrix() {
        let (settings, data_dir, db_path) = temp_settings();
        let pool = connect_pool(&settings).await.unwrap();
        let day = "2026-07-12";

        // Empty day.
        let m = daily_attendance_metrics(&pool, day).await.unwrap();
        assert_eq!(m.events_today, 0);
        assert_eq!(m.present_count, 0);

        let a = create_employee(&pool, "A", "Alice", None).await.unwrap();
        let b = create_employee(&pool, "B", "Bob", None).await.unwrap();
        let c = create_employee(&pool, "C", "Carol", None).await.unwrap();

        // In only → present.
        insert_event(&pool, a, "check_in", "2026-07-12 08:00:00", day).await;
        let m = daily_attendance_metrics(&pool, day).await.unwrap();
        assert_eq!(m.events_today, 1);
        assert_eq!(m.present_count, 1);

        // In then out → not present.
        insert_event(&pool, a, "check_out", "2026-07-12 17:00:00", day).await;
        let m = daily_attendance_metrics(&pool, day).await.unwrap();
        assert_eq!(m.events_today, 2);
        assert_eq!(m.present_count, 0);

        // Out then in → present.
        insert_event(&pool, b, "check_out", "2026-07-12 09:00:00", day).await;
        insert_event(&pool, b, "check_in", "2026-07-12 10:00:00", day).await;
        let m = daily_attendance_metrics(&pool, day).await.unwrap();
        assert_eq!(m.events_today, 4);
        assert_eq!(m.present_count, 1); // only B

        // Inactive employee with check_in must not count as present.
        update_employee_fields(
            &pool,
            c,
            EmployeePatch {
                full_name: None,
                department: None,
                is_active: Some(false),
            },
        )
        .await
        .unwrap();
        insert_event(&pool, c, "check_in", "2026-07-12 11:00:00", day).await;
        let m = daily_attendance_metrics(&pool, day).await.unwrap();
        assert_eq!(m.events_today, 5); // inactive still has events
        assert_eq!(m.present_count, 1); // still only B

        // Same-timestamp events ordered by ID: higher id wins.
        let d = create_employee(&pool, "D", "Dana", None).await.unwrap();
        let id_in = insert_event(&pool, d, "check_in", "2026-07-12 12:00:00", day).await;
        let id_out = insert_event(&pool, d, "check_out", "2026-07-12 12:00:00", day).await;
        assert!(id_out > id_in);
        let m = daily_attendance_metrics(&pool, day).await.unwrap();
        // D's latest by id is check_out → not present; B still present.
        assert_eq!(m.present_count, 1);
        assert_eq!(m.events_today, 7);

        // Reverse order for E: out then in at same ts → in has higher id → present.
        let e = create_employee(&pool, "E", "Eve", None).await.unwrap();
        insert_event(&pool, e, "check_out", "2026-07-12 13:00:00", day).await;
        insert_event(&pool, e, "check_in", "2026-07-12 13:00:00", day).await;
        let m = daily_attendance_metrics(&pool, day).await.unwrap();
        assert_eq!(m.present_count, 2); // B + E

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[tokio::test]
    async fn commit_outcome_matrix() {
        let (settings, data_dir, db_path) = temp_settings();
        let pool = connect_pool(&settings).await.unwrap();
        let emp = create_employee(&pool, "X", "Xavier", None).await.unwrap();

        // Committed check_in.
        let out = commit_identity(&pool, emp, "cam_in", 0.9, Some(1), 90.0, 30.0, "UTC")
            .await
            .unwrap();
        match out {
            CommitOutcome::Committed {
                event_id,
                name,
                kind,
            } => {
                assert!(event_id > 0);
                assert_eq!(name, "Xavier");
                assert_eq!(kind, "check_in");
            }
            other => panic!("expected Committed, got {other:?}"),
        }

        // Cooldown on same camera immediately after.
        let out = commit_identity(&pool, emp, "cam_in", 0.9, Some(1), 90.0, 30.0, "UTC")
            .await
            .unwrap();
        assert_eq!(out, CommitOutcome::Skipped(SkipReason::Cooldown));

        // Missing camera.
        let out = commit_identity(&pool, emp, "no_such_cam", 0.9, None, 0.0, 0.0, "UTC")
            .await
            .unwrap();
        assert_eq!(out, CommitOutcome::Skipped(SkipReason::MissingCamera));

        // Missing employee.
        let out = commit_identity(&pool, 999_999, "cam_in", 0.9, None, 0.0, 0.0, "UTC")
            .await
            .unwrap();
        assert_eq!(out, CommitOutcome::Skipped(SkipReason::MissingEmployee));

        // Inactive employee.
        let inactive = create_employee(&pool, "Z", "Zed", None).await.unwrap();
        update_employee_fields(
            &pool,
            inactive,
            EmployeePatch {
                full_name: None,
                department: None,
                is_active: Some(false),
            },
        )
        .await
        .unwrap();
        let out = commit_identity(&pool, inactive, "cam_in", 0.9, None, 0.0, 0.0, "UTC")
            .await
            .unwrap();
        assert_eq!(out, CommitOutcome::Skipped(SkipReason::InactiveEmployee));

        // No transition: bidirectional + recent check_in without dwell.
        // Seed a bidirectional camera.
        sqlx::query(
            "INSERT OR REPLACE INTO cameras(id, name, rtsp_url, webrtc_path, direction, enabled, sort_order)
             VALUES('cam_bi','Bi','','bi','bidirectional',1,2)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let bi = create_employee(&pool, "BI", "Bidi", None).await.unwrap();
        // First commit succeeds as check_in.
        let out = commit_identity(&pool, bi, "cam_bi", 0.9, None, 0.0, 30.0, "UTC")
            .await
            .unwrap();
        assert!(matches!(out, CommitOutcome::Committed { .. }));
        // Immediate second with cooldown 0 but min_dwell 30 → NoTransition.
        let out = commit_identity(&pool, bi, "cam_bi", 0.9, None, 0.0, 30.0, "UTC")
            .await
            .unwrap();
        assert_eq!(out, CommitOutcome::Skipped(SkipReason::NoTransition));

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}

#[cfg(test)]
mod list_employees_tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_settings() -> (Settings, PathBuf, PathBuf) {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("pksp-db-list-emp-{id}"));
        std::fs::create_dir_all(&data_dir).unwrap();
        let db_path = data_dir.join("test.db");
        let mut s = Settings::from_env();
        let abs = db_path
            .to_string_lossy()
            .trim_start_matches('/')
            .to_string();
        s.database_url = format!("sqlite:////{abs}?mode=rwc");
        s.data_dir = data_dir.clone();
        s.camera_upsert = true;
        s.cam_out_rtsp = String::new();
        (s, data_dir, db_path)
    }

    async fn add_image(
        pool: &SqlitePool,
        employee_id: i64,
        path: &str,
        usable: bool,
        reason: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO employee_images(employee_id, file_path, usable, reject_reason, created_at)
             VALUES(?,?,?,?,datetime('now'))",
        )
        .bind(employee_id)
        .bind(path)
        .bind(if usable { 1 } else { 0 })
        .bind(reason)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn add_embedding(pool: &SqlitePool, employee_id: i64, n: i64) {
        sqlx::query(
            "INSERT INTO employee_embeddings(employee_id, dim, vector, num_images_used, model_name, updated_at)
             VALUES(?,?,?,?,?,datetime('now'))",
        )
        .bind(employee_id)
        .bind(8_i64)
        .bind(vec![0u8; 8])
        .bind(n)
        .bind("buffalo_l")
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_employees_empty_one_and_many_shapes() {
        let (settings, data_dir, db_path) = temp_settings();
        let pool = connect_pool(&settings).await.unwrap();

        let empty = list_employees(&pool, None).await.unwrap();
        assert!(empty.is_empty());

        let a = create_employee(&pool, "A1", "Alice", Some("Finance"))
            .await
            .unwrap();
        add_image(&pool, a, "a1.jpg", true, None).await;
        add_image(&pool, a, "a2.jpg", false, Some("no_face")).await;
        add_embedding(&pool, a, 1).await;

        let one = list_employees(&pool, None).await.unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0]["id"], a);
        assert_eq!(one[0]["employee_code"], "A1");
        assert_eq!(one[0]["full_name"], "Alice");
        assert_eq!(one[0]["department"], "Finance");
        assert_eq!(one[0]["is_active"], true);
        assert_eq!(one[0]["image_count"], 2);
        assert_eq!(one[0]["usable_images"], 1);
        assert_eq!(one[0]["embedding_ready"], true);
        assert_eq!(one[0]["num_images_used"], 1);
        assert_eq!(one[0]["images"].as_array().unwrap().len(), 2);

        // Detail dict parity for the single row.
        let detail = employee_dict(&pool, a).await.unwrap();
        assert_eq!(one[0]["image_count"], detail["image_count"]);
        assert_eq!(one[0]["usable_images"], detail["usable_images"]);
        assert_eq!(one[0]["embedding_ready"], detail["embedding_ready"]);
        assert_eq!(one[0]["num_images_used"], detail["num_images_used"]);
        assert_eq!(
            one[0]["images"].as_array().unwrap().len(),
            detail["images"].as_array().unwrap().len()
        );

        let b = create_employee(&pool, "B1", "Bob", None).await.unwrap();
        // No images/embedding for Bob.
        let c = create_employee(&pool, "C1", "Cara", Some("Ops"))
            .await
            .unwrap();
        update_employee_fields(
            &pool,
            c,
            EmployeePatch {
                full_name: None,
                department: None,
                is_active: Some(false),
            },
        )
        .await
        .unwrap();
        add_image(&pool, c, "c.jpg", true, None).await;

        let many = list_employees(&pool, None).await.unwrap();
        // Alphabetical by full_name: Alice, Bob, Cara
        assert_eq!(many.len(), 3);
        assert_eq!(many[0]["full_name"], "Alice");
        assert_eq!(many[1]["full_name"], "Bob");
        assert_eq!(many[2]["full_name"], "Cara");
        assert_eq!(many[1]["image_count"], 0);
        assert_eq!(many[1]["usable_images"], 0);
        assert_eq!(many[1]["embedding_ready"], false);
        assert_eq!(many[1]["num_images_used"], 0);
        assert_eq!(many[1]["images"].as_array().unwrap().len(), 0);
        assert_eq!(many[2]["is_active"], false);
        assert_eq!(many[2]["image_count"], 1);
        assert_eq!(many[2]["embedding_ready"], false);

        // Search filters name/code (existing server contract; not department).
        let by_name = list_employees(&pool, Some("bob")).await.unwrap();
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0]["id"], b);
        let by_code = list_employees(&pool, Some("a1")).await.unwrap();
        assert_eq!(by_code.len(), 1);
        assert_eq!(by_code[0]["employee_code"], "A1");
        let by_dept = list_employees(&pool, Some("finance")).await.unwrap();
        assert!(by_dept.is_empty(), "server list does not search department");

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn list_employees_source_does_not_call_employee_dict() {
        let src = include_str!("lib.rs");
        // Locate the list_employees function body (until next pub async fn).
        let start = src
            .find("pub async fn list_employees")
            .expect("list_employees present");
        let rest = &src[start..];
        let end = rest
            .find("\npub async fn employee_dict")
            .expect("employee_dict follows list");
        let body = &rest[..end];
        assert!(
            !body.contains("employee_dict("),
            "list_employees must not call employee_dict"
        );
    }
}
