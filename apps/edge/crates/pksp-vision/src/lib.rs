//! Vision: buffalo_l ONNX engine, gallery, RTSP capture, worker pipeline.

mod align;
mod scrfd;
mod zones;

pub use align::{align_arcface_bgr, AlignError};
pub use scrfd::{
    decode_scrfd, letterbox_bgr_to_nchw, levels_from_heads, stride_for_score_len, ScrfdError,
    DEFAULT_SCORE_THRESH, NMS_IOU, STRIDES,
};

use pksp_core::{
    assign_tracks, commit_eligible, evaluate_vote, hud_state, match_top1, mean_l2_embedding,
    pack_embedding, pose_yaw_signed_approx, prefer_commit_track, quality_gate_extended,
    refine_hud_after_identity, should_vote, track_zone, trajectory_is_walkby, Detection, HudState,
    IdentityAttempt, MatchResult, SkipReason, TrackerState, ZoneMap,
};
use pksp_db::{
    attach_event_snapshot, commit_identity, daily_attendance_metrics, event_snapshot_rel_path,
    events_dir, load_gallery_matrix, local_date_str, CommitOutcome, Settings,
};
use serde_json::json;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, Semaphore};
use tracing::{info, warn};

/// Rate-limit metrics query failure logs (monotonic seconds of last emit).
static METRICS_ERR_LOG_SEC: AtomicU64 = AtomicU64::new(0);
pub use zones::load_zones_for_camera;

/// WebSocket / hub event (JSON object).
pub type WsEvent = serde_json::Value;
pub const VISION_MODEL: &str = "buffalo_l";

#[derive(Debug, Clone)]
pub struct DetectedFace {
    pub bbox: (f32, f32, f32, f32),
    pub det_score: f32,
    pub embedding: Vec<f32>,
    /// Optional 5-point landmarks (pixels) for pose / align.
    pub landmarks: Option<[[f32; 2]; 5]>,
}

/// Typed vision failure — never encode structural errors as an empty face list.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum FaceError {
    #[error("engine not ready")]
    NotReady,
    #[error("invalid input frame")]
    InvalidInput,
    #[error("model / session failure: {0}")]
    Model(String),
    #[error("SCRFD decode: {0}")]
    Decode(String),
    #[error("alignment: {0}")]
    Align(String),
    #[error("recognition output invalid")]
    BadEmbedding,
}

pub trait FaceEngine: Send + Sync {
    fn ready(&self) -> bool;
    fn model_name(&self) -> &str;
    fn execution_provider(&self) -> &str;
    /// `Ok(vec![])` means successful inference with zero faces. Errors are structural.
    fn detect_and_embed(
        &self,
        width: u32,
        height: u32,
        bgr: &[u8],
    ) -> Result<Vec<DetectedFace>, FaceError>;
}

/// ONNX buffalo_l engine — ready when model files load successfully.
pub struct OrtFaceEngine {
    pub ready: bool,
    pub provider: String,
    det_size: i32,
}

impl OrtFaceEngine {
    pub fn try_load(model_dir: &std::path::Path) -> Self {
        Self::try_load_with(model_dir, 640, "CPUExecutionProvider")
    }

    pub fn try_load_with(model_dir: &std::path::Path, det_size: i32, providers: &str) -> Self {
        let det = model_dir.join("det_10g.onnx");
        let rec = model_dir.join("w600k_r50.onnx");
        if det.is_file() && rec.is_file() {
            match try_init_ort(model_dir, det_size, providers) {
                Ok(provider) => Self {
                    ready: true,
                    provider,
                    det_size,
                },
                Err(e) => {
                    warn!("ONNX load failed ({e}); engine not ready");
                    Self {
                        ready: false,
                        provider: "unavailable".into(),
                        det_size,
                    }
                }
            }
        } else {
            Self {
                ready: false,
                provider: "unavailable".into(),
                det_size,
            }
        }
    }
}

/// Map a requested ONNX provider list to the provider actually applied.
///
/// CPU-first MVP: only `CPUExecutionProvider` is registered. Unsupported labels
/// fall back to CPU and are reported as such (never claim a requested EP we did
/// not enable).
pub fn applied_execution_provider(requested: &str) -> String {
    let first = requested
        .split(',')
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("CPUExecutionProvider");
    if first == "CPUExecutionProvider" {
        "CPUExecutionProvider".into()
    } else {
        warn!(
            requested = first,
            "ONNX provider not registered; using CPUExecutionProvider"
        );
        "CPUExecutionProvider".into()
    }
}

/// Attempt to validate model paths / init. Full SCRFD+ArcFace runs when ort sessions exist.
/// Returns the *applied* provider name on success.
fn try_init_ort(
    model_dir: &std::path::Path,
    _det_size: i32,
    providers: &str,
) -> Result<String, String> {
    let det = model_dir.join("det_10g.onnx");
    let rec = model_dir.join("w600k_r50.onnx");
    if !det.is_file() || !rec.is_file() {
        return Err("model files missing".into());
    }
    ort_sessions::load(model_dir, providers)
}

mod ort_sessions {
    use super::DetectedFace;
    use pksp_core::l2_normalize;
    use std::path::Path;
    use std::sync::Mutex;

    pub struct Sessions {
        pub det: ort::session::Session,
        pub rec: ort::session::Session,
        #[allow(dead_code)]
        pub det_size: i32,
        #[allow(dead_code)]
        pub provider: String,
    }

    static SESS: Mutex<Option<Sessions>> = Mutex::new(None);

    pub fn load(model_dir: &Path, providers: &str) -> Result<String, String> {
        let det_path = model_dir.join("det_10g.onnx");
        let rec_path = model_dir.join("w600k_r50.onnx");
        // Sessions use the default CPU EP; report only what is actually applied.
        let provider = super::applied_execution_provider(providers);

        let det = ort::session::Session::builder()
            .map_err(|e| e.to_string())?
            .commit_from_file(&det_path)
            .map_err(|e| e.to_string())?;
        let rec = ort::session::Session::builder()
            .map_err(|e| e.to_string())?
            .commit_from_file(&rec_path)
            .map_err(|e| e.to_string())?;

        let mut g = SESS.lock().map_err(|e| e.to_string())?;
        *g = Some(Sessions {
            det,
            rec,
            det_size: 640,
            provider: provider.clone(),
        });
        Ok(provider)
    }

    /// Full detect+embed via pure SCRFD + ArcFace alignment modules.
    pub fn detect_and_embed(
        width: u32,
        height: u32,
        bgr: &[u8],
        det_size: i32,
    ) -> Result<Vec<DetectedFace>, super::FaceError> {
        use super::FaceError;
        let mut g = SESS.lock().map_err(|e| FaceError::Model(e.to_string()))?;
        let s = g.as_mut().ok_or(FaceError::NotReady)?;
        run_pipeline(s, width, height, bgr, det_size)
    }

    fn run_pipeline(
        s: &mut Sessions,
        width: u32,
        height: u32,
        bgr: &[u8],
        det_size: i32,
    ) -> Result<Vec<DetectedFace>, super::FaceError> {
        use super::{align_arcface_bgr, FaceError};
        use crate::scrfd::{
            decode_scrfd, letterbox_bgr_to_nchw, levels_from_heads, DEFAULT_SCORE_THRESH, NMS_IOU,
        };

        if width == 0 || height == 0 || bgr.len() < (width * height * 3) as usize {
            return Err(FaceError::InvalidInput);
        }
        let ds = det_size as u32;
        let (tensor, meta) = letterbox_bgr_to_nchw(bgr, width, height, ds)
            .map_err(|e| FaceError::Decode(e.to_string()))?;
        let input = ort::value::Tensor::from_array(([1usize, 3, ds as usize, ds as usize], tensor))
            .map_err(|e| FaceError::Model(e.to_string()))?;

        // Collect score / bbox / kps heads by tensor length (names are export ids).
        let mut score_bufs: Vec<Vec<f32>> = Vec::new();
        let mut bbox_bufs: Vec<Vec<f32>> = Vec::new();
        let mut kps_bufs: Vec<Vec<f32>> = Vec::new();
        {
            let outputs = s
                .det
                .run(ort::inputs![input])
                .map_err(|e| FaceError::Model(e.to_string()))?;
            for (_name, val) in outputs.iter() {
                if let Ok((shape, data)) = val.try_extract_tensor::<f32>() {
                    let shape: Vec<i64> = shape.to_vec();
                    let flat: Vec<f32> = data.to_vec();
                    if flat.iter().any(|v| !v.is_finite()) {
                        return Err(FaceError::Decode("non-finite det tensor".into()));
                    }
                    // Classify by trailing dim / total length for buffalo_l
                    let last = *shape.last().unwrap_or(&0);
                    if last == 1 || shape.len() == 1 || (shape.len() == 2 && shape[1] == 1) {
                        score_bufs.push(flat);
                    } else if last == 4 {
                        bbox_bufs.push(flat);
                    } else if last == 10 {
                        kps_bufs.push(flat);
                    } else if flat.len() == 12800 || flat.len() == 3200 || flat.len() == 800 {
                        score_bufs.push(flat);
                    } else if flat.len() == 12800 * 4
                        || flat.len() == 3200 * 4
                        || flat.len() == 800 * 4
                    {
                        bbox_bufs.push(flat);
                    } else if flat.len() == 12800 * 10
                        || flat.len() == 3200 * 10
                        || flat.len() == 800 * 10
                    {
                        kps_bufs.push(flat);
                    }
                }
            }
        }

        if score_bufs.len() != 3 || bbox_bufs.len() != 3 || kps_bufs.len() != 3 {
            return Err(FaceError::Decode(format!(
                "expected 3 score/bbox/kps heads, got {}/{}/{}",
                score_bufs.len(),
                bbox_bufs.len(),
                kps_bufs.len()
            )));
        }
        let score_refs: Vec<&[f32]> = score_bufs.iter().map(|v| v.as_slice()).collect();
        let bbox_refs: Vec<&[f32]> = bbox_bufs.iter().map(|v| v.as_slice()).collect();
        let kps_refs: Vec<&[f32]> = kps_bufs.iter().map(|v| v.as_slice()).collect();
        let levels = levels_from_heads(&score_refs, &bbox_refs, &kps_refs, ds)
            .map_err(|e| FaceError::Decode(e.to_string()))?;
        let faces = decode_scrfd(&levels, &meta, DEFAULT_SCORE_THRESH, NMS_IOU)
            .map_err(|e| FaceError::Decode(e.to_string()))?;

        let mut out = Vec::new();
        for f in faces {
            let tensor = align_arcface_bgr(bgr, width, height, &f.landmarks)
                .map_err(|e| FaceError::Align(e.to_string()))?;
            let emb = embed_aligned(s, tensor)?;
            out.push(DetectedFace {
                bbox: f.bbox,
                det_score: f.score,
                embedding: emb,
                landmarks: Some(f.landmarks),
            });
        }
        Ok(out)
    }

    fn embed_aligned(s: &mut Sessions, tensor: Vec<f32>) -> Result<Vec<f32>, super::FaceError> {
        use super::FaceError;
        if tensor.len() != 3 * 112 * 112 || tensor.iter().any(|v| !v.is_finite()) {
            return Err(FaceError::BadEmbedding);
        }
        let input = ort::value::Tensor::from_array(([1usize, 3, 112, 112], tensor))
            .map_err(|e| FaceError::Model(e.to_string()))?;
        let outputs = s
            .rec
            .run(ort::inputs![input])
            .map_err(|e| FaceError::Model(e.to_string()))?;
        let mut emb_out: Option<Vec<f32>> = None;
        for (_name, val) in outputs.iter() {
            if let Ok((_shape, data)) = val.try_extract_tensor::<f32>() {
                emb_out = Some(data.to_vec());
                break;
            }
        }
        let emb = emb_out.ok_or(FaceError::BadEmbedding)?;
        if emb.len() != 512 || emb.iter().any(|v| !v.is_finite()) {
            return Err(FaceError::BadEmbedding);
        }
        Ok(l2_normalize(&emb))
    }
}

impl FaceEngine for OrtFaceEngine {
    fn ready(&self) -> bool {
        self.ready
    }
    fn model_name(&self) -> &str {
        VISION_MODEL
    }
    fn execution_provider(&self) -> &str {
        &self.provider
    }
    fn detect_and_embed(&self, w: u32, h: u32, bgr: &[u8]) -> Result<Vec<DetectedFace>, FaceError> {
        if !self.ready {
            return Err(FaceError::NotReady);
        }
        ort_sessions::detect_and_embed(w, h, bgr, self.det_size)
    }
}

pub struct Gallery {
    pub employee_ids: Vec<i64>,
    pub names: Vec<String>,
    pub matrix: Vec<Vec<f32>>,
    pub version: u64,
    pub threshold: f32,
    pub margin: f32,
}

impl Gallery {
    pub fn empty(threshold: f32, margin: f32) -> Self {
        Self {
            employee_ids: vec![],
            names: vec![],
            matrix: vec![],
            version: 0,
            threshold,
            margin,
        }
    }

    pub fn match_embedding(&self, emb: &[f32]) -> MatchResult {
        match_top1(
            emb,
            &self.matrix,
            &self.employee_ids,
            &self.names,
            self.threshold,
            self.margin,
        )
    }

    pub fn size(&self) -> usize {
        self.employee_ids.len()
    }
}

pub async fn reload_gallery(
    pool: &SqlitePool,
    gallery: &Arc<RwLock<Gallery>>,
    settings: &Settings,
) -> anyhow::Result<()> {
    let (ids, names, matrix) = load_gallery_matrix(
        pool,
        settings.embedding_dim,
        VISION_MODEL,
        settings.min_enroll_images,
    )
    .await?;
    let version = pksp_db::gallery_version(pool).await?;
    let mut g = gallery.write().unwrap();
    g.employee_ids = ids;
    g.names = names;
    g.matrix = matrix;
    g.version = version;
    g.threshold = settings.match_threshold;
    g.margin = settings.match_margin;
    Ok(())
}

/// Live camera online/FPS projection. Daily attendance counts come from SQLite
/// at emit time — never treat an in-memory counter as authoritative.
#[derive(Default)]
pub struct VisionMetrics {
    pub online: HashMap<String, bool>,
    pub fps: HashMap<String, f64>,
}

#[derive(Clone)]
pub struct VisionHandle {
    pub stop: Arc<AtomicBool>,
    pub metrics: Arc<RwLock<VisionMetrics>>,
    pub gallery_version: Arc<AtomicU64>,
}

impl VisionHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Start one RTSP vision loop per configured camera.
pub fn start_vision_worker(
    pool: SqlitePool,
    settings: Arc<Settings>,
    engine: Arc<dyn FaceEngine>,
    gallery: Arc<RwLock<Gallery>>,
    tx: broadcast::Sender<WsEvent>,
    camera_ids: Vec<String>,
    camera_rtsps: HashMap<String, String>,
) -> anyhow::Result<VisionHandle> {
    if !engine.ready() {
        anyhow::bail!("buffalo_l engine is not ready");
    }
    for camera_id in &camera_ids {
        if camera_rtsps
            .get(camera_id)
            .is_none_or(|url| url.trim().is_empty())
        {
            anyhow::bail!("enabled camera {camera_id} has no RTSP URL");
        }
    }

    let stop = Arc::new(AtomicBool::new(false));
    let metrics = Arc::new(RwLock::new(VisionMetrics::default()));
    let gallery_version = Arc::new(AtomicU64::new(0));
    let infer_sem = Arc::new(Semaphore::new(1));

    for cam_id in camera_ids {
        let stop_c = stop.clone();
        let pool_c = pool.clone();
        let settings_c = settings.clone();
        let engine_c = engine.clone();
        let gallery_c = gallery.clone();
        let tx_c = tx.clone();
        let metrics_c = metrics.clone();
        let zones = load_zones_for_camera(&settings.zone_config_dir, &cam_id);
        let zones_c = Arc::new(zones);
        let rtsp = camera_rtsps[&cam_id].clone();
        let sem = infer_sem.clone();
        let cam = cam_id.clone();
        tokio::spawn(async move {
            process_loop(
                cam, rtsp, stop_c, pool_c, settings_c, engine_c, gallery_c, tx_c, metrics_c,
                zones_c, sem,
            )
            .await;
        });
    }

    Ok(VisionHandle {
        stop,
        metrics,
        gallery_version,
    })
}

/// Latest captured frame — one slot only (drop-old policy). Sequence identifies observations.
#[derive(Clone)]
struct CapturedFrame {
    width: u32,
    height: u32,
    bgr: Vec<u8>,
    captured_at: Instant,
    sequence: u64,
}

type LatestFrame = Arc<RwLock<Option<CapturedFrame>>>;

/// True when this capture sequence has not yet been inferred.
/// Equality means the same observation (including after `wrapping_add` wrap).
#[inline]
fn should_infer_sequence(last_processed: Option<u64>, sequence: u64) -> bool {
    last_processed != Some(sequence)
}

#[allow(clippy::too_many_arguments)] // ponytail: orchestration boundary; group only when another caller exists
async fn process_loop(
    camera_id: String,
    rtsp_url: String,
    stop: Arc<AtomicBool>,
    pool: SqlitePool,
    settings: Arc<Settings>,
    engine: Arc<dyn FaceEngine>,
    gallery: Arc<RwLock<Gallery>>,
    tx: broadcast::Sender<WsEvent>,
    metrics: Arc<RwLock<VisionMetrics>>,
    zones: Arc<ZoneMap>,
    infer_sem: Arc<Semaphore>,
) {
    let mut tracker = TrackerState::new();
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut frames = 0u32;
    let mut fps_t0 = Instant::now();
    let mut target_fps = settings.vision_target_fps.max(0.5);
    let mut last_gallery_ver: u64 = gallery.read().unwrap().version;
    let mut last_processed_sequence: Option<u64> = None;
    let mut last_infer_err_log = Instant::now() - Duration::from_secs(60);

    // Shared latest frame; capture drops old frames instead of queueing latency.
    let latest: LatestFrame = Arc::new(RwLock::new(None));
    let stop_cap = stop.clone();
    let latest_c = latest.clone();
    let ffmpeg = settings.ffmpeg_bin.clone();
    let url = rtsp_url.clone();
    let cam = camera_id.clone();
    tokio::task::spawn_blocking(move || {
        capture_ffmpeg_loop(&cam, &url, &ffmpeg, stop_cap, latest_c);
    });

    {
        let mut m = metrics.write().unwrap();
        m.online.insert(camera_id.clone(), false);
    }

    while !stop.load(Ordering::SeqCst) {
        let interval = Duration::from_secs_f64(1.0 / target_fps);
        let elapsed = last.elapsed();
        if elapsed < interval {
            tokio::time::sleep(interval - elapsed).await;
        }
        last = Instant::now();

        // Gallery reload if version bumped
        if let Ok(db_ver) = pksp_db::gallery_version(&pool).await {
            if db_ver != last_gallery_ver {
                if let Err(e) = reload_gallery(&pool, &gallery, &settings).await {
                    warn!("gallery reload failed: {e}");
                } else {
                    last_gallery_ver = db_ver;
                    info!(camera_id = %camera_id, version = db_ver, "gallery reloaded");
                }
            }
        }

        let (w, h, bgr, captured_at) = match latest.read().unwrap().clone() {
            Some(frame) => {
                let age = frame.captured_at.elapsed().as_millis() as u64;
                let online = age < 2000;
                metrics
                    .write()
                    .unwrap()
                    .online
                    .insert(camera_id.clone(), online);
                if !online {
                    let _ = tx.send(json!({
                        "type": "camera_status",
                        "camera_id": camera_id,
                        "online": false,
                        "last_frame_age_ms": age,
                    }));
                    continue;
                }
                // Fresh-but-unchanged frame: publish status only; no detect/vote.
                if !should_infer_sequence(last_processed_sequence, frame.sequence) {
                    let _ = tx.send(json!({
                        "type": "camera_status",
                        "camera_id": camera_id,
                        "online": true,
                        "last_frame_age_ms": age,
                    }));
                    continue;
                }
                last_processed_sequence = Some(frame.sequence);
                (frame.width, frame.height, frame.bgr, frame.captured_at)
            }
            None => {
                metrics
                    .write()
                    .unwrap()
                    .online
                    .insert(camera_id.clone(), false);
                let _ = tx.send(json!({
                    "type": "camera_status",
                    "camera_id": camera_id,
                    "online": false,
                }));
                continue;
            }
        };

        let infer_t0 = Instant::now();
        let permit = match infer_sem.acquire().await {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Synchronous ORT off the async runtime; keep BGR for quality gates.
        let (bgr, raw) = match detect_faces_blocking(engine.clone(), w, h, bgr).await {
            Ok((pixels, Ok(faces))) => (pixels, faces),
            Ok((_pixels, Err(e))) => {
                // Structural model errors stay typed — never empty-face success.
                if last_infer_err_log.elapsed() >= Duration::from_secs(5) {
                    warn!(
                        camera_id = %camera_id,
                        error = %e,
                        "detect_and_embed failed"
                    );
                    last_infer_err_log = Instant::now();
                }
                drop(permit);
                continue;
            }
            Err(join_err) => {
                if last_infer_err_log.elapsed() >= Duration::from_secs(5) {
                    warn!(
                        camera_id = %camera_id,
                        error = %join_err,
                        "detect_and_embed join failed"
                    );
                    last_infer_err_log = Instant::now();
                }
                drop(permit);
                continue;
            }
        };

        let faces_out = process_detected_faces(
            &camera_id,
            w,
            h,
            &bgr,
            raw,
            &gallery,
            &mut tracker,
            &settings,
            &zones,
            &pool,
            &tx,
        )
        .await;
        drop(permit);
        let infer_ms = infer_t0.elapsed().as_secs_f64() * 1000.0;

        if settings.vision_adaptive {
            let budget = 1000.0 / target_fps;
            if infer_ms < budget * 0.6 && target_fps < 15.0 {
                target_fps = (target_fps + 0.5).min(15.0);
            } else if infer_ms > budget * 1.3 && target_fps > 1.0 {
                target_fps = (target_fps - 0.5).max(1.0);
            }
        }

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let age_ms = captured_at.elapsed().as_millis() as u64;
        let _ = tx.send(json!({
            "type": "detections",
            "camera_id": camera_id,
            "ts": ts,
            "frame_w": w,
            "frame_h": h,
            "faces": faces_out,
        }));
        let _ = tx.send(json!({
            "type": "camera_status",
            "camera_id": camera_id,
            "online": true,
            "last_frame_age_ms": age_ms,
        }));

        // Only successful inference passes count toward processed FPS.
        frames += 1;
        if fps_t0.elapsed() >= Duration::from_secs(2) {
            let fps = frames as f64 / fps_t0.elapsed().as_secs_f64();
            metrics.write().unwrap().fps.insert(camera_id.clone(), fps);
            frames = 0;
            fps_t0 = Instant::now();
            // ponytail: 1-2 cameras; centralize only if measured DB load matters
            emit_persisted_metrics(&pool, &settings, &metrics, &tx).await;
        }
    }
}

/// Fill the existing WS metrics contract from SQLite for the configured local date.
///
/// On DB/timezone/query error: rate-limited warning and skip this emission entirely
/// (never broadcast a zero-filled invented fallback).
async fn emit_persisted_metrics(
    pool: &SqlitePool,
    settings: &Settings,
    metrics: &Arc<RwLock<VisionMetrics>>,
    tx: &broadcast::Sender<WsEvent>,
) {
    let day = match local_date_str(chrono::Utc::now(), &settings.app_timezone) {
        Ok(d) => d,
        Err(e) => {
            warn_metrics_error(&e.to_string());
            return;
        }
    };
    let daily = match daily_attendance_metrics(pool, &day).await {
        Ok(d) => d,
        Err(e) => {
            warn_metrics_error(&e.to_string());
            return;
        }
    };
    let m = metrics.read().unwrap();
    let online_n = m.online.values().filter(|v| **v).count();
    let _ = tx.send(json!({
        "type": "metrics",
        "cameras_online": online_n,
        "present_count": daily.present_count,
        "events_today": daily.events_today,
        "vision_fps": m.fps.clone(),
    }));
}

fn warn_metrics_error(msg: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let prev = METRICS_ERR_LOG_SEC.load(Ordering::Relaxed);
    if now.saturating_sub(prev) >= 30 {
        METRICS_ERR_LOG_SEC.store(now, Ordering::Relaxed);
        // Sanitized: do not include SQL text or paths beyond the error Display.
        warn!(error = %msg, "daily metrics query failed; skipping metrics emit");
    }
}

/// Blocking ffmpeg RTSP → raw BGR24 frames into latest buffer.
fn capture_ffmpeg_loop(
    camera_id: &str,
    rtsp_url: &str,
    ffmpeg_bin: &str,
    stop: Arc<AtomicBool>,
    latest: LatestFrame,
) {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let w = 640u32;
    let h = 360u32;
    let frame_size = (w * h * 3) as usize;
    let mut sequence: u64 = 0;
    while !stop.load(Ordering::SeqCst) {
        let bin = resolve_ffmpeg(ffmpeg_bin);
        info!(camera_id, bin = %bin, "starting ffmpeg RTSP capture");
        let mut child = match Command::new(&bin)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-rtsp_transport",
                "tcp",
                "-i",
                rtsp_url,
                "-an",
                "-vf",
                &format!("scale={w}:{h}"),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "bgr24",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                warn!(camera_id, "ffmpeg spawn failed: {e}");
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        };
        let mut stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        let mut buf = vec![0u8; frame_size];
        loop {
            if stop.load(Ordering::SeqCst) {
                let _ = child.kill();
                break;
            }
            match stdout.read_exact(&mut buf) {
                Ok(()) => {
                    sequence = sequence.wrapping_add(1);
                    *latest.write().unwrap() = Some(CapturedFrame {
                        width: w,
                        height: h,
                        bgr: buf.clone(),
                        captured_at: Instant::now(),
                        sequence,
                    });
                }
                Err(_) => {
                    warn!(camera_id, "ffmpeg pipe ended; reconnecting");
                    let _ = child.kill();
                    break;
                }
            }
        }
        let _ = child.wait();
        if stop.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn resolve_ffmpeg(name: &str) -> String {
    if name != "ffmpeg" && std::path::Path::new(name).is_file() {
        return name.to_string();
    }
    let edge_bin = bundled_bin_path("ffmpeg");
    if edge_bin.is_file() {
        return edge_bin.display().to_string();
    }
    name.to_string()
}

fn bundled_bin_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bin")
        .join(name)
}

/// Run quality/match/track/DB/broadcast for faces already detected off-thread.
#[allow(clippy::too_many_arguments)] // ponytail: orchestration boundary; group only when another caller exists
async fn process_detected_faces(
    camera_id: &str,
    w: u32,
    h: u32,
    bgr: &[u8],
    raw: Vec<DetectedFace>,
    gallery: &Arc<RwLock<Gallery>>,
    tracker: &mut TrackerState,
    settings: &Settings,
    zones: &ZoneMap,
    pool: &SqlitePool,
    tx: &broadcast::Sender<WsEvent>,
) -> Vec<serde_json::Value> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    let smart = settings.enable_smart_scene;
    let zones_empty = zones.zones.is_empty();

    let dets: Vec<Detection> = {
        let g = gallery.read().unwrap();
        let mut dets = Vec::new();
        for f in raw {
            let gray = crop_gray_luma(bgr, w, h, f.bbox);
            let q = quality_gate_extended(
                f.det_score,
                f.bbox,
                settings.min_det_score,
                settings.min_face_px,
                w as i32,
                h as i32,
                false,
                f.landmarks.as_ref(),
                gray.as_ref().map(|(g, gw, gh)| (g.as_slice(), *gw, *gh)),
                settings.pose_max_yaw,
                settings.blur_min_var,
                0.0,
                255.0, // exposure disabled unless settings expose bounds
            );
            let bbox_n = (
                f.bbox.0 / w as f32,
                f.bbox.1 / h as f32,
                f.bbox.2 / w as f32,
                f.bbox.3 / h as f32,
            );
            let zone = track_zone(bbox_n, &zones.zones);
            let may_vote = q.ok && should_vote(zone, smart, zones_empty);
            let mut emp_id = None;
            let mut label = if q.ok {
                "UNKNOWN".to_string()
            } else {
                "LOW QUALITY".to_string()
            };
            let mut score = 0.0f32;
            if q.ok && g.size() > 0 {
                let m = g.match_embedding(&f.embedding);
                emp_id = m.employee_id;
                label = m.label;
                score = m.score;
            }
            let is_walkby = false; // updated after track assign
            let state = hud_state(q.ok, emp_id, zone, is_walkby, smart, zones_empty).as_str();
            dets.push(Detection {
                bbox: bbox_n,
                employee_id: emp_id,
                score,
                label,
                // only quality_ok faces that may vote append history
                quality_ok: may_vote,
                ts,
                state: state.into(),
            });
            // Preserve true quality for HUD when ignore suppresses vote
            if q.ok && !may_vote {
                // keep quality_ok false for voting; label still shows identity if matched
                let _ = &f;
            }
        }
        dets
    };

    let tracks = assign_tracks(
        tracker,
        &dets,
        settings.iou_match_threshold,
        settings.track_max_age_frames,
        settings.vote_window,
    );

    // Only prefer one track for commit when multi-face
    let preferred_id = prefer_commit_track(&tracks, &zones.zones, smart).map(|t| t.track_id);

    let mut out = Vec::new();
    for tr in &tracks {
        let zone = track_zone(tr.bbox, &zones.zones);
        let is_walkby = trajectory_is_walkby(tr, &zones.zones, settings.walkby_min_dwell_frames);
        // quality_ok on track already encodes may_vote
        let mut hud = hud_state(
            tr.quality_ok || tr.label == "LOW QUALITY",
            tr.employee_id,
            zone,
            is_walkby,
            smart,
            zones_empty,
        );
        if !tr.quality_ok && tr.label == "LOW QUALITY" {
            hud = HudState::LowQuality;
        }

        let can_try_commit = tr.quality_ok
            && tr.employee_id.is_some()
            && preferred_id.map(|id| id == tr.track_id).unwrap_or(true);

        let mut attempt = IdentityAttempt::NotAttempted;
        let mut eligible = false;

        if can_try_commit {
            if let Some(commit) = evaluate_vote(
                tr,
                settings.vote_window,
                settings.vote_min_hits,
                settings.match_threshold,
            ) {
                eligible = commit_eligible(
                    tr,
                    &zones.zones,
                    settings.enable_smart_scene,
                    settings.walkby_min_dwell_frames,
                );
                if eligible {
                    match commit_identity(
                        pool,
                        commit.employee_id,
                        camera_id,
                        commit.avg_score,
                        Some(tr.track_id),
                        settings.cooldown_seconds,
                        settings.min_dwell_seconds,
                        &settings.app_timezone,
                    )
                    .await
                    {
                        Ok(CommitOutcome::Committed {
                            event_id,
                            name,
                            kind,
                        }) => {
                            let event_ts = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs_f64();
                            attempt = IdentityAttempt::Committed;
                            if let Some(t) = tracker
                                .tracks
                                .iter_mut()
                                .find(|t| t.track_id == tr.track_id)
                            {
                                t.last_commit_ts = Some(ts);
                                t.state = "committed".into();
                            }
                            // Best-effort snapshot: never roll back the attendance event.
                            let bbox_n = [
                                tr.bbox.0.clamp(0.0, 1.0),
                                tr.bbox.1.clamp(0.0, 1.0),
                                tr.bbox.2.clamp(0.0, 1.0),
                                tr.bbox.3.clamp(0.0, 1.0),
                            ];
                            let (snapshot_url, bbox_wire) = match persist_event_snapshot(
                                pool, settings, event_id, w, h, bgr, bbox_n,
                            )
                            .await
                            {
                                Ok(_rel) => (
                                    Some(format!("/api/attendance/events/{event_id}/snapshot")),
                                    Some(bbox_n),
                                ),
                                Err(e) => {
                                    warn!(
                                        event_id,
                                        error = %e,
                                        "event snapshot persist failed; attendance kept"
                                    );
                                    (None, None)
                                }
                            };
                            let mut msg = json!({
                                "type": "attendance",
                                "event_id": event_id,
                                "employee_id": commit.employee_id,
                                "name": name,
                                "kind": kind,
                                "camera_id": camera_id,
                                "score": commit.avg_score,
                                "ts": event_ts,
                            });
                            msg["snapshot_url"] = match snapshot_url {
                                Some(url) => json!(url),
                                None => json!(null),
                            };
                            msg["bbox"] = match bbox_wire {
                                Some(b) => json!([b[0], b[1], b[2], b[3]]),
                                None => json!(null),
                            };
                            let _ = tx.send(msg);
                        }
                        Ok(CommitOutcome::Skipped(reason)) => {
                            attempt = IdentityAttempt::Skipped(reason);
                            if reason != SkipReason::Cooldown {
                                // Sanitized operator breadcrumb — no PII beyond reason code.
                                tracing::debug!(
                                    reason = reason.as_str(),
                                    camera_id,
                                    "identity commit skipped"
                                );
                            }
                        }
                        Err(e) => warn!("commit failed: {e}"),
                    }
                }
            }
        }

        hud = refine_hud_after_identity(hud, is_walkby, eligible, attempt);
        let state = hud.as_str().to_string();
        out.push(json!({
            "track_id": tr.track_id,
            "bbox": [tr.bbox.0, tr.bbox.1, tr.bbox.2, tr.bbox.3],
            "label": tr.label,
            "employee_id": tr.employee_id,
            "score": tr.score,
            "quality_ok": tr.quality_ok,
            "state": state,
        }));
    }
    out
}

/// Encode a BGR frame as a bounded JPEG (max long edge 640) for event snapshots.
/// Returns encoded JPEG bytes.
pub fn encode_event_snapshot_jpeg(w: u32, h: u32, bgr: &[u8]) -> Result<Vec<u8>, String> {
    if w == 0 || h == 0 {
        return Err("empty frame".into());
    }
    let expected = (w as usize).saturating_mul(h as usize).saturating_mul(3);
    if bgr.len() < expected {
        return Err("bgr buffer too short".into());
    }
    // BGR → RGB
    let mut rgb = Vec::with_capacity(expected);
    for chunk in bgr[..expected].chunks_exact(3) {
        rgb.push(chunk[2]);
        rgb.push(chunk[1]);
        rgb.push(chunk[0]);
    }
    let img =
        image::RgbImage::from_raw(w, h, rgb).ok_or_else(|| "rgb rebuild failed".to_string())?;
    // Bound long edge to 640 (plan: bounded 640×360-class JPEG).
    const MAX_LONG: u32 = 640;
    let long = w.max(h);
    let scaled = if long > MAX_LONG {
        let scale = MAX_LONG as f32 / long as f32;
        let nw = ((w as f32) * scale).round().max(1.0) as u32;
        let nh = ((h as f32) * scale).round().max(1.0) as u32;
        image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    scaled
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| format!("jpeg encode: {e}"))?;
    Ok(buf)
}

/// Best-effort: write JPEG under DATA_DIR/events/<id>.jpg and attach DB metadata.
/// On DB failure after write, removes the file. Never undoes the attendance event.
pub async fn persist_event_snapshot(
    pool: &SqlitePool,
    settings: &Settings,
    event_id: i64,
    w: u32,
    h: u32,
    bgr: &[u8],
    bbox_n: [f32; 4],
) -> anyhow::Result<String> {
    let rel = event_snapshot_rel_path(event_id);
    let dir = events_dir(settings);
    let abs = settings.data_dir.join(&rel);
    let tmp = abs.with_extension("jpg.tmp");
    let frame = bgr.to_vec();
    let write_abs = abs.clone();
    let write_tmp = tmp.clone();

    // JPEG resize/encode and filesystem I/O are blocking. Keep them off the
    // Tokio worker that is also serving WebSockets and API requests.
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        std::fs::create_dir_all(&dir)?;
        let jpeg = encode_event_snapshot_jpeg(w, h, &frame).map_err(|e| anyhow::anyhow!(e))?;
        let result = (|| -> anyhow::Result<()> {
            std::fs::write(&write_tmp, &jpeg)?;
            std::fs::rename(&write_tmp, &write_abs)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&write_tmp);
        }
        result
    })
    .await
    .map_err(|e| anyhow::anyhow!("snapshot writer join failed: {e}"))??;

    if let Err(e) = attach_event_snapshot(pool, event_id, &rel, bbox_n).await {
        let _ = tokio::fs::remove_file(&abs).await;
        return Err(e);
    }
    Ok(rel)
}

/// Crop bbox to grayscale luma for blur/exposure quality extensions.
pub fn crop_gray_luma(
    bgr: &[u8],
    w: u32,
    h: u32,
    bbox: (f32, f32, f32, f32),
) -> Option<(Vec<u8>, usize, usize)> {
    let x1 = bbox.0.max(0.0).floor() as u32;
    let y1 = bbox.1.max(0.0).floor() as u32;
    let x2 = bbox.2.min(w as f32).ceil() as u32;
    let y2 = bbox.3.min(h as f32).ceil() as u32;
    if x2 <= x1 || y2 <= y1 {
        return None;
    }
    let cw = (x2 - x1) as usize;
    let ch = (y2 - y1) as usize;
    let mut gray = Vec::with_capacity(cw * ch);
    for y in y1..y2 {
        for x in x1..x2 {
            let i = ((y * w + x) * 3) as usize;
            if i + 2 >= bgr.len() {
                return None;
            }
            let b = bgr[i] as f32;
            let g = bgr[i + 1] as f32;
            let r = bgr[i + 2] as f32;
            gray.push((0.114 * b + 0.587 * g + 0.299 * r) as u8);
        }
    }
    Some((gray, cw, ch))
}

/// Ordinary per-image analysis outcome (not a systemic failure).
#[derive(Debug, Clone)]
struct ImageAnalysis {
    /// Client-facing filename (original upload name or basename of stored path).
    filename: String,
    existing_id: Option<i64>,
    /// Absolute staged path for a new upload (under employee dir).
    staged_abs: Option<std::path::PathBuf>,
    /// Relative final path for a new upload after promote.
    final_rel: Option<String>,
    usable: bool,
    reason: Option<String>,
    embedding: Option<Vec<f32>>,
}

/// Validate enrollment image bytes: format, dimensions, pixel budget.
/// Returns detected format extension (`jpg`/`png`/`webp`).
pub fn validate_enroll_image_bytes(data: &[u8], settings: &Settings) -> Result<String, String> {
    if data.is_empty() {
        return Err("empty image".into());
    }
    if data.len() > settings.max_enroll_file_bytes {
        return Err(format!(
            "file exceeds max size of {} bytes",
            settings.max_enroll_file_bytes
        ));
    }
    let format =
        image::guess_format(data).map_err(|_| "unsupported or corrupt image".to_string())?;
    let ext = match format {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::WebP => "webp",
        _ => return Err("unsupported image format; use JPEG, PNG, or WebP".into()),
    };
    // Dimension probe without full decode allocation of the pixel buffer.
    let reader = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .map_err(|_| "unsupported or corrupt image".to_string())?;
    let (w, h) = reader
        .into_dimensions()
        .map_err(|_| "unsupported or corrupt image".to_string())?;
    if w > settings.max_enroll_image_dim || h > settings.max_enroll_image_dim {
        return Err(format!(
            "image dimensions {w}x{h} exceed max {}",
            settings.max_enroll_image_dim
        ));
    }
    let pixels = (w as u64).saturating_mul(h as u64);
    if pixels > settings.max_enroll_pixels {
        return Err(format!(
            "image pixel count {pixels} exceeds max {}",
            settings.max_enroll_pixels
        ));
    }
    // Full decode under the same limits (guards corrupt payloads that pass headers).
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(settings.max_enroll_image_dim);
    limits.max_image_height = Some(settings.max_enroll_image_dim);
    limits.max_alloc = Some(settings.max_enroll_pixels.saturating_mul(4));
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .map_err(|_| "unsupported or corrupt image".to_string())?;
    reader.limits(limits);
    let _ = reader
        .decode()
        .map_err(|_| "unsupported or corrupt image".to_string())?;
    Ok(ext.to_string())
}

/// Browser-guided enrollment preview: quality + pose only — never embeddings.
#[derive(Debug, Clone, PartialEq)]
pub struct EnrollmentPreview {
    pub accepted: bool,
    pub reason: Option<String>,
    /// Normalized xyxy bbox in [0,1], when exactly one face was detected.
    pub bbox: Option<[f32; 4]>,
    /// Signed yaw degrees when landmarks available; `None` if no face or no landmarks.
    pub yaw: Option<f32>,
    pub face_count: usize,
}

/// Analyze one candidate enrollment frame for guided capture.
///
/// Uses the same size/detection/blur/pose gates as enrollment. Returns no embeddings.
/// Ordinary rejections (no face, multiple faces, quality) are `Ok` with `accepted: false`.
/// Structural engine failures are `Err`.
pub fn analyze_enrollment_preview(
    engine: &dyn FaceEngine,
    settings: &Settings,
    data: &[u8],
) -> Result<EnrollmentPreview, FaceError> {
    if !engine.ready() {
        return Err(FaceError::NotReady);
    }
    validate_enroll_image_bytes(data, settings).map_err(|_| FaceError::InvalidInput)?;
    let (w, h, bgr) = decode_to_bgr(data).map_err(|_| FaceError::InvalidInput)?;
    let faces = engine.detect_and_embed(w, h, &bgr)?;
    let face_count = faces.len();
    if face_count == 0 {
        return Ok(EnrollmentPreview {
            accepted: false,
            reason: Some("no_face".into()),
            bbox: None,
            yaw: None,
            face_count: 0,
        });
    }
    if face_count > 1 {
        return Ok(EnrollmentPreview {
            accepted: false,
            reason: Some("multiple_faces".into()),
            bbox: None,
            yaw: None,
            face_count,
        });
    }
    let face = &faces[0];
    if face.embedding.iter().any(|v| !v.is_finite()) {
        return Err(FaceError::BadEmbedding);
    }
    let bbox_n = [
        (face.bbox.0 / w as f32).clamp(0.0, 1.0),
        (face.bbox.1 / h as f32).clamp(0.0, 1.0),
        (face.bbox.2 / w as f32).clamp(0.0, 1.0),
        (face.bbox.3 / h as f32).clamp(0.0, 1.0),
    ];
    let yaw = face.landmarks.as_ref().map(pose_yaw_signed_approx);
    let gray = crop_gray_luma(&bgr, w, h, face.bbox);
    let q = quality_gate_extended(
        face.det_score,
        face.bbox,
        settings.min_det_score,
        settings.min_face_px,
        w as i32,
        h as i32,
        false,
        face.landmarks.as_ref(),
        gray.as_ref().map(|(g, gw, gh)| (g.as_slice(), *gw, *gh)),
        settings.pose_max_yaw,
        settings.blur_min_var,
        0.0,
        255.0,
    );
    Ok(EnrollmentPreview {
        accepted: q.ok,
        reason: q.reason,
        bbox: Some(bbox_n),
        yaw,
        face_count: 1,
    })
}

fn analyze_bgr_image(
    engine: &dyn FaceEngine,
    settings: &Settings,
    w: u32,
    h: u32,
    bgr: &[u8],
) -> Result<(Option<Vec<f32>>, Option<String>), FaceError> {
    let faces = engine.detect_and_embed(w, h, bgr)?;
    if faces.is_empty() {
        return Ok((None, Some("no_face".into())));
    }
    if faces.len() > 1 {
        return Ok((None, Some("multiple_faces".into())));
    }
    let face = &faces[0];
    if face.embedding.iter().any(|v| !v.is_finite()) {
        return Err(FaceError::BadEmbedding);
    }
    let gray = crop_gray_luma(bgr, w, h, face.bbox);
    let q = quality_gate_extended(
        face.det_score,
        face.bbox,
        settings.min_det_score,
        settings.min_face_px,
        w as i32,
        h as i32,
        false,
        face.landmarks.as_ref(),
        gray.as_ref().map(|(g, gw, gh)| (g.as_slice(), *gw, *gh)),
        settings.pose_max_yaw,
        settings.blur_min_var,
        0.0,
        255.0,
    );
    if !q.ok {
        Ok((None, q.reason))
    } else {
        Ok((Some(face.embedding.clone()), None))
    }
}

fn decode_to_bgr(data: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let im = image::load_from_memory(data).map_err(|_| "decode_error".to_string())?;
    let rgb = im.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut bgr = Vec::with_capacity((w * h * 3) as usize);
    for p in rgb.pixels() {
        bgr.push(p[2]);
        bgr.push(p[1]);
        bgr.push(p[0]);
    }
    Ok((w, h, bgr))
}

fn short_uid() -> String {
    let uid = uuid::Uuid::new_v4().simple().to_string();
    uid[..12.min(uid.len())].to_string()
}

/// Analyze one image file on disk or in memory. Systemic FaceError is returned;
/// missing/corrupt *existing* files become ordinary rejections.
#[allow(clippy::too_many_arguments)] // private analysis helper; groups would obscure existing vs staged paths
fn analyze_one(
    engine: &dyn FaceEngine,
    settings: &Settings,
    filename: &str,
    existing_id: Option<i64>,
    staged_abs: Option<std::path::PathBuf>,
    final_rel: Option<String>,
    bytes: Option<&[u8]>,
    abs_path: Option<&std::path::Path>,
    is_new_upload: bool,
) -> Result<ImageAnalysis, FaceError> {
    let load = if let Some(b) = bytes {
        decode_to_bgr(b)
    } else if let Some(p) = abs_path {
        match std::fs::read(p) {
            Ok(data) => decode_to_bgr(&data),
            Err(_) => Err("unreadable".into()),
        }
    } else {
        Err("missing".into())
    };

    match load {
        Ok((w, h, bgr)) => match analyze_bgr_image(engine, settings, w, h, &bgr) {
            Ok((emb, reason)) => Ok(ImageAnalysis {
                filename: filename.to_string(),
                existing_id,
                staged_abs,
                final_rel,
                usable: emb.is_some(),
                reason,
                embedding: emb,
            }),
            Err(e) => Err(e),
        },
        Err(code) => {
            if is_new_upload {
                // New uploads are validated before staging; decode here is systemic.
                Err(FaceError::InvalidInput)
            } else {
                Ok(ImageAnalysis {
                    filename: filename.to_string(),
                    existing_id,
                    staged_abs: None,
                    final_rel: None,
                    usable: false,
                    reason: Some(code),
                    embedding: None,
                })
            }
        }
    }
}

/// Enroll or recompute: reanalyze existing rows plus optional new files; one transactional persist.
///
/// New file bytes must already pass [`validate_enroll_image_bytes`]. `new_files` empty ⇒ recompute.
/// Gallery reload runs after commit when `gallery` is provided; failures are non-fatal and surface
/// as `gallery_reload_pending: true`.
pub async fn enroll_images(
    pool: &SqlitePool,
    settings: &Settings,
    engine: Arc<dyn FaceEngine>,
    employee_id: i64,
    new_files: Vec<(String, Vec<u8>)>,
    gallery: Option<&Arc<RwLock<Gallery>>>,
) -> anyhow::Result<serde_json::Value> {
    let received = new_files.len();
    let dest = settings.enroll_dir().join(employee_id.to_string());
    std::fs::create_dir_all(&dest)?;

    // Stage new files first (UUID staging names). Cleaned on any pre-commit failure.
    let mut staged_paths: Vec<std::path::PathBuf> = Vec::new();
    let mut staged_meta: Vec<(String, std::path::PathBuf, String)> = Vec::new();
    // (filename, staged_abs, final_rel)
    for (filename, data) in &new_files {
        let ext = validate_enroll_image_bytes(data, settings)
            .map_err(|e| anyhow::anyhow!("validation: {e}"))?;
        let uid = short_uid();
        let staged = dest.join(format!(".staging-{uid}.{ext}"));
        let final_rel = format!("enroll/{employee_id}/{uid}.{ext}");
        std::fs::write(&staged, data)?;
        staged_paths.push(staged.clone());
        staged_meta.push((filename.clone(), staged, final_rel));
    }

    let existing = pksp_db::list_employee_images(pool, employee_id).await?;

    // Blocking analysis of all images (existing + staged) — plan 005 boundary.
    let settings_c = settings.clone();
    let engine_c = engine.clone();
    let analysis_join = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for row in &existing {
            let abs = settings_c.data_dir.join(&row.file_path);
            let name = std::path::Path::new(&row.file_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(row.file_path.as_str())
                .to_string();
            match analyze_one(
                engine_c.as_ref(),
                &settings_c,
                &name,
                Some(row.id),
                None,
                None,
                None,
                Some(&abs),
                false,
            ) {
                Ok(a) => out.push(a),
                Err(e) => return Err(e),
            }
        }
        for (filename, staged, final_rel) in &staged_meta {
            match analyze_one(
                engine_c.as_ref(),
                &settings_c,
                filename,
                None,
                Some(staged.clone()),
                Some(final_rel.clone()),
                None,
                Some(staged.as_path()),
                true,
            ) {
                Ok(a) => out.push(a),
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    })
    .await;

    let analyses = match analysis_join {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => {
            for p in &staged_paths {
                let _ = std::fs::remove_file(p);
            }
            anyhow::bail!("enrollment analysis failed: {e}");
        }
        Err(join_e) => {
            for p in &staged_paths {
                let _ = std::fs::remove_file(p);
            }
            anyhow::bail!("enrollment analysis join failed: {join_e}");
        }
    };

    // Promote staged → final paths before DB commit (recovery deletes only new files).
    let mut promoted_finals: Vec<std::path::PathBuf> = Vec::new();
    let promote = (|| -> anyhow::Result<()> {
        for a in &analyses {
            if let (Some(staged), Some(rel)) = (&a.staged_abs, &a.final_rel) {
                let final_abs = settings.data_dir.join(rel);
                if let Some(parent) = final_abs.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if let Err(e) = std::fs::rename(staged, &final_abs) {
                    // Cross-device rename fallback.
                    std::fs::copy(staged, &final_abs).map_err(|_| e)?;
                    let _ = std::fs::remove_file(staged);
                }
                promoted_finals.push(final_abs);
            }
        }
        Ok(())
    })();
    if let Err(e) = promote {
        for p in &staged_paths {
            let _ = std::fs::remove_file(p);
        }
        for p in &promoted_finals {
            let _ = std::fs::remove_file(p);
        }
        return Err(e);
    }

    let vectors: Vec<Vec<f32>> = analyses
        .iter()
        .filter_map(|a| a.embedding.clone())
        .collect();
    let usable_count = vectors.len();
    let model_name = engine.model_name().to_string();
    let min_n = settings.min_enroll_images;
    let dim = settings.embedding_dim;

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            for p in &promoted_finals {
                let _ = std::fs::remove_file(p);
            }
            for p in &staged_paths {
                let _ = std::fs::remove_file(p);
            }
            return Err(e.into());
        }
    };

    let db_result: anyhow::Result<()> = async {
        for a in &analyses {
            if let Some(id) = a.existing_id {
                pksp_db::update_employee_image_tx(&mut tx, id, a.usable, a.reason.as_deref())
                    .await?;
            } else if let Some(rel) = &a.final_rel {
                pksp_db::add_employee_image_tx(
                    &mut tx,
                    employee_id,
                    rel,
                    a.usable,
                    a.reason.as_deref(),
                )
                .await?;
            }
        }

        if usable_count >= min_n {
            let mean = mean_l2_embedding(&vectors, dim)?;
            let blob = pack_embedding(&mean, dim)?;
            pksp_db::save_embedding_tx(
                &mut tx,
                employee_id,
                &blob,
                dim,
                usable_count as i32,
                &model_name,
            )
            .await?;
        } else {
            pksp_db::delete_embedding_tx(&mut tx, employee_id).await?;
        }
        pksp_db::bump_gallery_version_tx(&mut tx).await?;
        Ok(())
    }
    .await;

    if let Err(e) = db_result {
        let _ = tx.rollback().await;
        for p in &promoted_finals {
            let _ = std::fs::remove_file(p);
        }
        for p in &staged_paths {
            let _ = std::fs::remove_file(p);
        }
        return Err(e);
    }
    if let Err(e) = tx.commit().await {
        for p in &promoted_finals {
            let _ = std::fs::remove_file(p);
        }
        return Err(e.into());
    }

    // Remove any leftover staging files after successful promote.
    for p in &staged_paths {
        let _ = std::fs::remove_file(p);
    }

    let embedding_ready = usable_count >= min_n;
    let mut rejected = Vec::new();
    let mut results = Vec::new();
    for a in &analyses {
        results.push(json!({
            "filename": a.filename,
            "usable": a.usable,
            "reason": a.reason,
        }));
        if !a.usable {
            rejected.push(json!({
                "filename": a.filename,
                "reason": a.reason.clone().unwrap_or_else(|| "rejected".into()),
            }));
        }
    }

    let mut gallery_reload_pending = false;
    if let Some(g) = gallery {
        match reload_gallery(pool, g, settings).await {
            Ok(()) => {}
            Err(e1) => match reload_gallery(pool, g, settings).await {
                Ok(()) => {}
                Err(e2) => {
                    warn!(
                        employee_id,
                        error = %e2,
                        first_error = %e1,
                        "gallery reload failed after enrollment commit"
                    );
                    gallery_reload_pending = true;
                }
            },
        }
    }

    info!(
        "enroll employee={employee_id} received={received} usable={usable_count} ready={embedding_ready} reload_pending={gallery_reload_pending}"
    );

    Ok(json!({
        "received": received,
        "usable": usable_count,
        "rejected": rejected,
        "embedding_ready": embedding_ready,
        "num_images_used": if embedding_ready { usable_count } else { 0 },
        "results": results,
        "gallery_reload_pending": gallery_reload_pending,
    }))
}

/// Run engine detection on a blocking thread pool worker; preserve BGR ownership.
async fn detect_faces_blocking(
    engine: Arc<dyn FaceEngine>,
    width: u32,
    height: u32,
    bgr: Vec<u8>,
) -> Result<(Vec<u8>, Result<Vec<DetectedFace>, FaceError>), FaceError> {
    tokio::task::spawn_blocking(move || {
        let result = if engine.ready() {
            engine.detect_and_embed(width, height, &bgr)
        } else {
            Ok(vec![])
        };
        (bgr, result)
    })
    .await
    .map_err(|e| FaceError::Model(format!("inference join failed: {e}")))
}

#[cfg(test)]
mod frame_scheduling_tests {
    use super::*;
    use pksp_core::{assign_tracks, Detection, TrackerState};
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    struct CountingEngine {
        calls: AtomicUsize,
        dim: usize,
    }

    impl CountingEngine {
        fn new(dim: usize) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                dim,
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(AtomicOrdering::SeqCst)
        }
    }

    impl FaceEngine for CountingEngine {
        fn ready(&self) -> bool {
            true
        }
        fn model_name(&self) -> &str {
            "counting"
        }
        fn execution_provider(&self) -> &str {
            "test"
        }
        fn detect_and_embed(
            &self,
            width: u32,
            height: u32,
            _bgr: &[u8],
        ) -> Result<Vec<DetectedFace>, FaceError> {
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            let x1 = width as f32 * 0.2;
            let y1 = height as f32 * 0.2;
            let x2 = width as f32 * 0.8;
            let y2 = height as f32 * 0.8;
            Ok(vec![DetectedFace {
                bbox: (x1, y1, x2, y2),
                det_score: 0.99,
                embedding: vec![0.0; self.dim],
                landmarks: None,
            }])
        }
    }

    struct SleepyEngine {
        delay: Duration,
    }

    impl FaceEngine for SleepyEngine {
        fn ready(&self) -> bool {
            true
        }
        fn model_name(&self) -> &str {
            "sleepy"
        }
        fn execution_provider(&self) -> &str {
            "test"
        }
        fn detect_and_embed(
            &self,
            _w: u32,
            _h: u32,
            _bgr: &[u8],
        ) -> Result<Vec<DetectedFace>, FaceError> {
            std::thread::sleep(self.delay);
            Ok(vec![])
        }
    }

    struct PanicEngine;

    impl FaceEngine for PanicEngine {
        fn ready(&self) -> bool {
            true
        }
        fn model_name(&self) -> &str {
            "panic"
        }
        fn execution_provider(&self) -> &str {
            "test"
        }
        fn detect_and_embed(
            &self,
            _w: u32,
            _h: u32,
            _bgr: &[u8],
        ) -> Result<Vec<DetectedFace>, FaceError> {
            panic!("deliberate inference panic");
        }
    }

    struct FailingEngine;

    impl FaceEngine for FailingEngine {
        fn ready(&self) -> bool {
            true
        }
        fn model_name(&self) -> &str {
            "fail"
        }
        fn execution_provider(&self) -> &str {
            "test"
        }
        fn detect_and_embed(
            &self,
            _w: u32,
            _h: u32,
            _bgr: &[u8],
        ) -> Result<Vec<DetectedFace>, FaceError> {
            Err(FaceError::Model("session broken".into()))
        }
    }

    /// Simulate process_loop ticks over a fixed latest-frame slot (no queue).
    fn simulate_ticks(
        engine: &CountingEngine,
        sequences: &[u64],
        tracker: &mut TrackerState,
    ) -> (usize, usize) {
        let mut last_processed: Option<u64> = None;
        let mut processed = 0usize;
        let mut votes_appended = 0usize;
        for &seq in sequences {
            if !should_infer_sequence(last_processed, seq) {
                continue;
            }
            last_processed = Some(seq);
            let w = 100u32;
            let h = 100u32;
            let bgr = vec![128u8; (w * h * 3) as usize];
            let faces = engine.detect_and_embed(w, h, &bgr).expect("detect");
            processed += 1;
            let before: usize = tracker.tracks.iter().map(|t| t.history.len()).sum();
            let dets: Vec<Detection> = faces
                .iter()
                .map(|f| Detection {
                    bbox: (
                        f.bbox.0 / w as f32,
                        f.bbox.1 / h as f32,
                        f.bbox.2 / w as f32,
                        f.bbox.3 / h as f32,
                    ),
                    employee_id: Some(1),
                    score: 0.9,
                    label: "T".into(),
                    quality_ok: true,
                    ts: 0.0,
                    state: "matched".into(),
                })
                .collect();
            let _tracks = assign_tracks(tracker, &dets, 0.3, 10, 5);
            let after: usize = tracker.tracks.iter().map(|t| t.history.len()).sum();
            votes_appended += after.saturating_sub(before);
        }
        (processed, votes_appended)
    }

    #[test]
    fn duplicate_sequence_infers_once_and_one_vote() {
        let engine = CountingEngine::new(8);
        let mut tracker = TrackerState::new();
        // Three polls of sequence 1, then a new sequence 2.
        let (processed, votes) = simulate_ticks(&engine, &[1, 1, 1, 2], &mut tracker);
        assert_eq!(
            engine.calls(),
            2,
            "engine must run once per unique sequence"
        );
        assert_eq!(processed, 2);
        // One vote per processed sequence (not per poll).
        assert_eq!(votes, 2, "at most one vote per processed sequence");
    }

    #[tokio::test]
    async fn worker_rejects_missing_rtsp_before_spawning() {
        let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let settings = Arc::new(Settings::from_env());
        let engine: Arc<dyn FaceEngine> = Arc::new(CountingEngine::new(512));
        let gallery = Arc::new(RwLock::new(Gallery::empty(0.75, 0.10)));
        let (tx, _) = broadcast::channel(1);
        let result = start_vision_worker(
            pool,
            settings,
            engine,
            gallery,
            tx,
            vec!["cam_in".into()],
            HashMap::new(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn wrapping_sequence_equality_is_not_new() {
        assert!(!should_infer_sequence(Some(u64::MAX), u64::MAX));
        assert!(should_infer_sequence(Some(u64::MAX), 0));
        assert!(should_infer_sequence(None, 0));
    }

    #[test]
    fn applied_provider_reports_cpu_for_unsupported() {
        assert_eq!(
            applied_execution_provider("CPUExecutionProvider"),
            "CPUExecutionProvider"
        );
        assert_eq!(
            applied_execution_provider("OpenVINOExecutionProvider,CPUExecutionProvider"),
            "CPUExecutionProvider"
        );
        assert_eq!(
            applied_execution_provider("CUDAExecutionProvider"),
            "CPUExecutionProvider"
        );
        assert_eq!(applied_execution_provider(""), "CPUExecutionProvider");
    }

    #[tokio::test]
    async fn blocking_detect_allows_timer_to_advance() {
        let engine: Arc<dyn FaceEngine> = Arc::new(SleepyEngine {
            delay: Duration::from_millis(200),
        });
        let bgr = vec![40u8; 64 * 64 * 3];
        let start = Instant::now();
        let handle = tokio::spawn(detect_faces_blocking(engine, 64, 64, bgr));
        // Independent future must complete while inference still sleeps.
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(
            start.elapsed() < Duration::from_millis(150),
            "tokio timer stalled by blocking inference"
        );
        let out = handle.await.expect("join spawn");
        assert!(out.is_ok());
        assert!(start.elapsed() >= Duration::from_millis(180));
    }

    #[tokio::test]
    async fn panic_in_blocking_detect_is_join_error() {
        let engine: Arc<dyn FaceEngine> = Arc::new(PanicEngine);
        let bgr = vec![40u8; 32 * 32 * 3];
        let err = detect_faces_blocking(engine, 32, 32, bgr)
            .await
            .expect_err("panic must surface as FaceError");
        match err {
            FaceError::Model(msg) => assert!(msg.contains("join"), "{msg}"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn typed_model_failure_is_not_empty_success() {
        let engine: Arc<dyn FaceEngine> = Arc::new(FailingEngine);
        let bgr = vec![40u8; 32 * 32 * 3];
        let (pixels, result) = detect_faces_blocking(engine, 32, 32, bgr)
            .await
            .expect("join ok");
        assert_eq!(pixels.len(), 32 * 32 * 3);
        match result {
            Err(FaceError::Model(msg)) => assert!(msg.contains("session"), "{msg}"),
            Ok(v) => panic!("must not translate model failure to Ok({} faces)", v.len()),
            Err(other) => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn latest_frame_slot_replaces_not_queues() {
        let latest: LatestFrame = Arc::new(RwLock::new(None));
        {
            let mut g = latest.write().unwrap();
            *g = Some(CapturedFrame {
                width: 10,
                height: 10,
                bgr: vec![1; 300],
                captured_at: Instant::now(),
                sequence: 1,
            });
            *g = Some(CapturedFrame {
                width: 10,
                height: 10,
                bgr: vec![2; 300],
                captured_at: Instant::now(),
                sequence: 2,
            });
        }
        let snap = latest.read().unwrap().clone().unwrap();
        assert_eq!(snap.sequence, 2);
        assert_eq!(snap.bgr[0], 2);
    }

    /// Semaphore is released even when detection panics (via join error path).
    #[tokio::test]
    async fn semaphore_released_after_join_failure() {
        let sem = Arc::new(Semaphore::new(1));
        let engine: Arc<dyn FaceEngine> = Arc::new(PanicEngine);
        let bgr = vec![1u8; 16 * 16 * 3];
        {
            let _permit = sem.acquire().await.unwrap();
            let _ = detect_faces_blocking(engine, 16, 16, bgr).await;
            // permit drops here
        }
        // Must be acquirable immediately — no deadlock.
        let got = tokio::time::timeout(Duration::from_millis(100), sem.acquire()).await;
        assert!(got.is_ok(), "semaphore not released after join failure");
    }

    #[test]
    fn duplicate_polls_do_not_inflate_processed_count() {
        let mut last: Option<u64> = None;
        let mut processed = 0u32;
        for seq in [7u64, 7, 7, 8, 8] {
            if should_infer_sequence(last, seq) {
                last = Some(seq);
                processed += 1;
            }
        }
        assert_eq!(processed, 2);
    }
}

#[cfg(test)]
mod enroll_tests {
    use super::*;
    use image::{ImageBuffer, ImageFormat, Rgb};
    use pksp_db::{connect_pool, create_employee, list_employee_images, Settings};
    use std::io::Cursor;
    use std::path::PathBuf;
    use uuid::Uuid;

    struct TempData {
        dir: PathBuf,
    }

    impl Drop for TempData {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn temp_settings(min_enroll: usize) -> (TempData, Settings) {
        let dir = std::env::temp_dir().join(format!("pksp-enroll-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("t.db");
        let abs = db_path
            .to_string_lossy()
            .trim_start_matches('/')
            .to_string();
        let mut s = Settings::from_env();
        s.database_url = format!("sqlite:////{abs}?mode=rwc");
        s.data_dir = dir.clone();
        s.min_enroll_images = min_enroll;
        s.embedding_dim = 16;
        s.min_face_px = 10;
        s.min_det_score = 0.1;
        s.pose_max_yaw = 0.0;
        s.blur_min_var = 0.0;
        s.max_enroll_files = 10;
        s.max_enroll_file_bytes = 5_242_880;
        s.max_enroll_upload_bytes = 33_554_432;
        s.max_enroll_image_dim = 4096;
        s.max_enroll_pixels = 20_000_000;
        (TempData { dir }, s)
    }

    fn solid_png(w: u32, h: u32, v: u8) -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |_, _| Rgb([v, v, v]));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .unwrap();
        buf
    }

    struct SingleFaceEngine {
        dim: usize,
    }

    impl FaceEngine for SingleFaceEngine {
        fn ready(&self) -> bool {
            true
        }
        fn model_name(&self) -> &str {
            VISION_MODEL
        }
        fn execution_provider(&self) -> &str {
            "test"
        }
        fn detect_and_embed(
            &self,
            width: u32,
            height: u32,
            bgr: &[u8],
        ) -> Result<Vec<DetectedFace>, FaceError> {
            let mean = bgr.iter().map(|&x| x as f32).sum::<f32>() / bgr.len().max(1) as f32;
            if mean < 5.0 {
                return Ok(vec![]);
            }
            let mut embedding = vec![0.1; self.dim];
            embedding[0] = mean / 255.0;
            Ok(vec![DetectedFace {
                bbox: (10.0, 10.0, width as f32 - 10.0, height as f32 - 10.0),
                det_score: 0.99,
                embedding: pksp_core::l2_normalize(&embedding),
                landmarks: None,
            }])
        }
    }

    struct MultiFaceEngine {
        dim: usize,
    }

    impl FaceEngine for MultiFaceEngine {
        fn ready(&self) -> bool {
            true
        }
        fn model_name(&self) -> &str {
            "multi"
        }
        fn execution_provider(&self) -> &str {
            "test"
        }
        fn detect_and_embed(
            &self,
            width: u32,
            height: u32,
            _bgr: &[u8],
        ) -> Result<Vec<DetectedFace>, FaceError> {
            let emb = vec![0.1; self.dim];
            Ok(vec![
                DetectedFace {
                    bbox: (10.0, 10.0, 80.0, 80.0),
                    det_score: 0.99,
                    embedding: emb.clone(),
                    landmarks: None,
                },
                DetectedFace {
                    bbox: (
                        width as f32 * 0.5,
                        height as f32 * 0.5,
                        width as f32 * 0.9,
                        height as f32 * 0.9,
                    ),
                    det_score: 0.95,
                    embedding: emb,
                    landmarks: None,
                },
            ])
        }
    }

    struct BoomEngine;

    impl FaceEngine for BoomEngine {
        fn ready(&self) -> bool {
            true
        }
        fn model_name(&self) -> &str {
            "boom"
        }
        fn execution_provider(&self) -> &str {
            "test"
        }
        fn detect_and_embed(
            &self,
            _w: u32,
            _h: u32,
            _bgr: &[u8],
        ) -> Result<Vec<DetectedFace>, FaceError> {
            Err(FaceError::Model("tensor broken".into()))
        }
    }

    #[test]
    fn validate_rejects_over_dimension_and_corrupt() {
        let (_t, s) = temp_settings(1);
        let big = solid_png(100, 100, 200);
        // tighten dim
        let mut s2 = s.clone();
        s2.max_enroll_image_dim = 50;
        assert!(validate_enroll_image_bytes(&big, &s2).is_err());
        assert!(validate_enroll_image_bytes(b"not-an-image", &s).is_err());
        assert!(validate_enroll_image_bytes(&solid_png(64, 64, 180), &s).is_ok());
    }

    #[test]
    fn preview_accepts_single_face_with_normalized_bbox() {
        let (_t, settings) = temp_settings(1);
        let engine = SingleFaceEngine {
            dim: settings.embedding_dim,
        };
        let png = solid_png(80, 80, 120);
        let prev = analyze_enrollment_preview(&engine, &settings, &png).unwrap();
        assert!(prev.accepted);
        assert_eq!(prev.face_count, 1);
        assert!(prev.reason.is_none());
        let bbox = prev.bbox.expect("bbox");
        assert!(bbox[0] >= 0.0 && bbox[2] <= 1.0);
        assert!(bbox[1] >= 0.0 && bbox[3] <= 1.0);
        assert!(bbox[2] > bbox[0] && bbox[3] > bbox[1]);
        // SingleFaceEngine has no landmarks
        assert!(prev.yaw.is_none());
    }

    #[test]
    fn preview_rejects_no_face() {
        let (_t, settings) = temp_settings(1);
        let engine = SingleFaceEngine {
            dim: settings.embedding_dim,
        };
        // near-black → SingleFaceEngine returns empty
        let png = solid_png(80, 80, 0);
        let prev = analyze_enrollment_preview(&engine, &settings, &png).unwrap();
        assert!(!prev.accepted);
        assert_eq!(prev.reason.as_deref(), Some("no_face"));
        assert_eq!(prev.face_count, 0);
        assert!(prev.bbox.is_none());
    }

    #[test]
    fn preview_rejects_multiple_faces() {
        let (_t, settings) = temp_settings(1);
        let engine = MultiFaceEngine {
            dim: settings.embedding_dim,
        };
        let png = solid_png(80, 80, 120);
        let prev = analyze_enrollment_preview(&engine, &settings, &png).unwrap();
        assert!(!prev.accepted);
        assert_eq!(prev.reason.as_deref(), Some("multiple_faces"));
        assert_eq!(prev.face_count, 2);
        assert!(prev.bbox.is_none());
    }

    #[test]
    fn preview_systemic_engine_failure() {
        let (_t, settings) = temp_settings(1);
        let engine = BoomEngine;
        let png = solid_png(80, 80, 120);
        let err = analyze_enrollment_preview(&engine, &settings, &png).unwrap_err();
        assert!(matches!(err, FaceError::Model(_)));
    }

    #[test]
    fn preview_invalid_payload() {
        let (_t, settings) = temp_settings(1);
        let engine = SingleFaceEngine {
            dim: settings.embedding_dim,
        };
        let err = analyze_enrollment_preview(&engine, &settings, b"not-an-image").unwrap_err();
        assert_eq!(err, FaceError::InvalidInput);
    }

    struct SignedYawEngine {
        dim: usize,
        landmarks: [[f32; 2]; 5],
    }

    impl FaceEngine for SignedYawEngine {
        fn ready(&self) -> bool {
            true
        }
        fn model_name(&self) -> &str {
            VISION_MODEL
        }
        fn execution_provider(&self) -> &str {
            "test"
        }
        fn detect_and_embed(
            &self,
            width: u32,
            height: u32,
            _bgr: &[u8],
        ) -> Result<Vec<DetectedFace>, FaceError> {
            let emb = vec![0.1; self.dim];
            Ok(vec![DetectedFace {
                bbox: (10.0, 10.0, width as f32 - 10.0, height as f32 - 10.0),
                det_score: 0.99,
                embedding: pksp_core::l2_normalize(&emb),
                landmarks: Some(self.landmarks),
            }])
        }
    }

    #[test]
    fn preview_returns_signed_yaw() {
        let (_t, mut settings) = temp_settings(1);
        // Allow large yaw so quality does not reject — we only check signed value.
        settings.pose_max_yaw = 90.0;
        // Nose right of eye mid → positive
        let engine = SignedYawEngine {
            dim: settings.embedding_dim,
            landmarks: [
                [20.0, 40.0],
                [50.0, 40.0],
                [55.0, 60.0],
                [25.0, 80.0],
                [50.0, 80.0],
            ],
        };
        let png = solid_png(80, 80, 120);
        let prev = analyze_enrollment_preview(&engine, &settings, &png).unwrap();
        let yaw = prev.yaw.expect("yaw");
        assert!(yaw > 0.0, "expected positive signed yaw, got {yaw}");
    }

    #[test]
    fn encode_event_snapshot_jpeg_bounds_and_decodes() {
        let w = 800u32;
        let h = 450u32;
        let mut bgr = vec![0u8; (w * h * 3) as usize];
        for (i, px) in bgr.chunks_exact_mut(3).enumerate() {
            px[0] = (i % 200) as u8;
            px[1] = 40;
            px[2] = 80;
        }
        let jpeg = encode_event_snapshot_jpeg(w, h, &bgr).unwrap();
        assert!(!jpeg.is_empty());
        let im = image::load_from_memory(&jpeg).unwrap();
        assert!(im.width() <= 640);
        assert!(im.height() <= 640);
        // aspect roughly preserved
        let aspect = im.width() as f32 / im.height() as f32;
        assert!((aspect - (800.0 / 450.0)).abs() < 0.05);
    }

    #[test]
    fn encode_event_snapshot_rejects_short_buffer() {
        assert!(encode_event_snapshot_jpeg(10, 10, &[0u8; 10]).is_err());
    }

    #[tokio::test]
    async fn two_batches_are_cumulative() {
        let (_t, settings) = temp_settings(1);
        let pool = connect_pool(&settings).await.unwrap();
        let eid = create_employee(&pool, "E1", "Alice", None).await.unwrap();
        let engine: Arc<dyn FaceEngine> = Arc::new(SingleFaceEngine {
            dim: settings.embedding_dim,
        });
        let gallery = Arc::new(RwLock::new(Gallery::empty(0.45, 0.08)));

        let a = solid_png(80, 80, 120);
        let r1 = enroll_images(
            &pool,
            &settings,
            engine.clone(),
            eid,
            vec![("a.png".into(), a)],
            Some(&gallery),
        )
        .await
        .unwrap();
        assert_eq!(r1["received"], 1);
        assert_eq!(r1["usable"], 1);
        assert_eq!(r1["embedding_ready"], true);
        assert_eq!(r1["num_images_used"], 1);
        assert_eq!(r1["gallery_reload_pending"], false);
        assert!(r1["results"].as_array().unwrap().len() == 1);

        let b = solid_png(80, 80, 130);
        let r2 = enroll_images(
            &pool,
            &settings,
            engine.clone(),
            eid,
            vec![("b.png".into(), b)],
            Some(&gallery),
        )
        .await
        .unwrap();
        assert_eq!(r2["received"], 1);
        assert_eq!(r2["usable"], 2, "mean must use A+B: {r2}");
        assert_eq!(r2["num_images_used"], 2);
        assert_eq!(r2["embedding_ready"], true);

        let rows = list_employee_images(&pool, eid).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(gallery.read().unwrap().size(), 1);
    }

    #[tokio::test]
    async fn recompute_preserves_ids_and_paths() {
        let (_t, settings) = temp_settings(1);
        let pool = connect_pool(&settings).await.unwrap();
        let eid = create_employee(&pool, "E2", "Bob", None).await.unwrap();
        let engine: Arc<dyn FaceEngine> = Arc::new(SingleFaceEngine {
            dim: settings.embedding_dim,
        });

        enroll_images(
            &pool,
            &settings,
            engine.clone(),
            eid,
            vec![
                ("x.png".into(), solid_png(80, 80, 140)),
                ("y.png".into(), solid_png(80, 80, 150)),
            ],
            None,
        )
        .await
        .unwrap();
        let before = list_employee_images(&pool, eid).await.unwrap();
        assert_eq!(before.len(), 2);
        let paths_before: Vec<_> = before.iter().map(|r| r.file_path.clone()).collect();
        let ids_before: Vec<_> = before.iter().map(|r| r.id).collect();
        let file_count_before = std::fs::read_dir(settings.enroll_dir().join(eid.to_string()))
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_file())
            .count();

        let r = enroll_images(&pool, &settings, engine, eid, vec![], None)
            .await
            .unwrap();
        assert_eq!(r["received"], 0);
        assert_eq!(r["usable"], 2);

        let after = list_employee_images(&pool, eid).await.unwrap();
        assert_eq!(after.iter().map(|r| r.id).collect::<Vec<_>>(), ids_before);
        assert_eq!(
            after
                .iter()
                .map(|r| r.file_path.clone())
                .collect::<Vec<_>>(),
            paths_before
        );
        let file_count_after = std::fs::read_dir(settings.enroll_dir().join(eid.to_string()))
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_file())
            .count();
        assert_eq!(file_count_before, file_count_after);
        // No duplicate files
        for p in &paths_before {
            assert!(settings.data_dir.join(p).is_file());
        }
    }

    #[tokio::test]
    async fn below_minimum_deletes_embedding() {
        let (_t, settings) = temp_settings(2);
        let pool = connect_pool(&settings).await.unwrap();
        let eid = create_employee(&pool, "E3", "Cara", None).await.unwrap();
        let engine: Arc<dyn FaceEngine> = Arc::new(SingleFaceEngine {
            dim: settings.embedding_dim,
        });

        // One usable image → embedding_ready false, no row
        let r = enroll_images(
            &pool,
            &settings,
            engine.clone(),
            eid,
            vec![("only.png".into(), solid_png(80, 80, 160))],
            None,
        )
        .await
        .unwrap();
        assert_eq!(r["usable"], 1);
        assert_eq!(r["embedding_ready"], false);
        assert!(!pksp_db::embedding_exists(&pool, eid).await.unwrap());

        // Reach minimum
        enroll_images(
            &pool,
            &settings,
            engine.clone(),
            eid,
            vec![("two.png".into(), solid_png(80, 80, 170))],
            None,
        )
        .await
        .unwrap();
        assert!(pksp_db::embedding_exists(&pool, eid).await.unwrap());

        // Replace analysis with dark (no face) by writing over files would be hard;
        // instead recompute after manually marking — use dark image only upload path:
        // upload a dark image and set min so that only dark new ones don't help.
        // Delete usability by recompute with corrupt existing file:
        let rows = list_employee_images(&pool, eid).await.unwrap();
        for row in &rows {
            let abs = settings.data_dir.join(&row.file_path);
            std::fs::write(&abs, b"not-png-anymore").unwrap();
        }
        let r2 = enroll_images(&pool, &settings, engine, eid, vec![], None)
            .await
            .unwrap();
        assert_eq!(r2["usable"], 0);
        assert_eq!(r2["embedding_ready"], false);
        assert!(!pksp_db::embedding_exists(&pool, eid).await.unwrap());
    }

    #[tokio::test]
    async fn systemic_model_error_preserves_prior_state() {
        let (_t, settings) = temp_settings(1);
        let pool = connect_pool(&settings).await.unwrap();
        let eid = create_employee(&pool, "E4", "Dan", None).await.unwrap();
        let ok_engine: Arc<dyn FaceEngine> = Arc::new(SingleFaceEngine {
            dim: settings.embedding_dim,
        });
        enroll_images(
            &pool,
            &settings,
            ok_engine,
            eid,
            vec![("keep.png".into(), solid_png(80, 80, 180))],
            None,
        )
        .await
        .unwrap();
        let ver = pksp_db::gallery_version(&pool).await.unwrap();
        let rows = list_employee_images(&pool, eid).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert!(pksp_db::embedding_exists(&pool, eid).await.unwrap());
        let path = settings.data_dir.join(&rows[0].file_path);
        assert!(path.is_file());
        let file_bytes = std::fs::read(&path).unwrap();

        let boom: Arc<dyn FaceEngine> = Arc::new(BoomEngine);
        let err = enroll_images(
            &pool,
            &settings,
            boom,
            eid,
            vec![("new.png".into(), solid_png(80, 80, 190))],
            None,
        )
        .await;
        assert!(err.is_err(), "systemic model error must abort");

        let rows2 = list_employee_images(&pool, eid).await.unwrap();
        assert_eq!(rows2.len(), 1);
        assert_eq!(rows2[0].id, rows[0].id);
        assert_eq!(rows2[0].file_path, rows[0].file_path);
        assert!(pksp_db::embedding_exists(&pool, eid).await.unwrap());
        assert_eq!(pksp_db::gallery_version(&pool).await.unwrap(), ver);
        assert_eq!(std::fs::read(&path).unwrap(), file_bytes);
        // No leftover staging/new files
        let enroll_dir = settings.enroll_dir().join(eid.to_string());
        let names: Vec<_> = std::fs::read_dir(&enroll_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names.len(),
            1,
            "only original file should remain: {names:?}"
        );
        assert!(!names.iter().any(|n| n.contains("staging")));
    }

    #[tokio::test]
    async fn ordinary_no_face_and_multi_face_recorded() {
        let (_t, settings) = temp_settings(1);
        let pool = connect_pool(&settings).await.unwrap();
        let eid = create_employee(&pool, "E5", "Eve", None).await.unwrap();
        let test_engine: Arc<dyn FaceEngine> = Arc::new(SingleFaceEngine {
            dim: settings.embedding_dim,
        });
        // Dark image → no_face via deterministic test engine.
        let r = enroll_images(
            &pool,
            &settings,
            test_engine,
            eid,
            vec![("dark.png".into(), solid_png(80, 80, 0))],
            None,
        )
        .await
        .unwrap();
        assert_eq!(r["usable"], 0);
        assert_eq!(r["results"][0]["usable"], false);
        assert_eq!(r["results"][0]["reason"], "no_face");
        assert_eq!(r["results"][0]["filename"], "dark.png");

        let multi: Arc<dyn FaceEngine> = Arc::new(MultiFaceEngine {
            dim: settings.embedding_dim,
        });
        let r2 = enroll_images(
            &pool,
            &settings,
            multi,
            eid,
            vec![("m.png".into(), solid_png(100, 100, 200))],
            None,
        )
        .await
        .unwrap();
        let last = r2["results"].as_array().unwrap().last().unwrap();
        assert_eq!(last["usable"], false);
        assert_eq!(last["reason"], "multiple_faces");
    }

    #[test]
    fn validate_rejects_over_pixels_and_file_bytes() {
        let (_t, mut s) = temp_settings(1);
        s.max_enroll_pixels = 100;
        assert!(validate_enroll_image_bytes(&solid_png(20, 20, 100), &s).is_err());
        s.max_enroll_pixels = 20_000_000;
        s.max_enroll_file_bytes = 10;
        assert!(validate_enroll_image_bytes(&solid_png(32, 32, 100), &s).is_err());
    }
}

#[cfg(test)]
mod metrics_and_hud_tests {
    use super::*;
    use pksp_core::{refine_hud_after_identity, HudState, IdentityAttempt, SkipReason};
    use pksp_db::{connect_pool, create_employee, daily_attendance_metrics, Settings};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_settings() -> (Settings, PathBuf, PathBuf) {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!("pksp-vision-metrics-{id}"));
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

    #[tokio::test]
    async fn emit_persisted_metrics_from_sqlite() {
        let (settings, data_dir, db_path) = temp_settings();
        let pool = connect_pool(&settings).await.unwrap();
        let day = pksp_db::local_date_str(chrono::Utc::now(), "UTC").unwrap();
        let emp = create_employee(&pool, "M1", "Metrics", None).await.unwrap();
        sqlx::query(
            "INSERT INTO attendance_events(employee_id, camera_id, kind, score, ts, local_date)
             VALUES(?,?,?,?,?,?)",
        )
        .bind(emp)
        .bind("cam_in")
        .bind("check_in")
        .bind(0.9_f64)
        .bind(format!("{day} 08:00:00"))
        .bind(&day)
        .execute(&pool)
        .await
        .unwrap();

        let metrics = Arc::new(RwLock::new(VisionMetrics::default()));
        metrics
            .write()
            .unwrap()
            .online
            .insert("cam_in".into(), true);
        metrics.write().unwrap().fps.insert("cam_in".into(), 5.0);
        let (tx, mut rx) = broadcast::channel::<WsEvent>(8);

        emit_persisted_metrics(&pool, &settings, &metrics, &tx).await;
        let msg = rx.try_recv().expect("metrics message");
        assert_eq!(msg["type"], "metrics");
        assert_eq!(msg["present_count"], 1);
        assert_eq!(msg["events_today"], 1);
        assert_eq!(msg["cameras_online"], 1);
        assert_eq!(msg["vision_fps"]["cam_in"], 5.0);

        // Restart simulation: fresh metrics handle still reads persisted truth.
        let metrics2 = Arc::new(RwLock::new(VisionMetrics::default()));
        let (tx2, mut rx2) = broadcast::channel::<WsEvent>(8);
        emit_persisted_metrics(&pool, &settings, &metrics2, &tx2).await;
        let msg2 = rx2.try_recv().expect("metrics after reset");
        assert_eq!(msg2["present_count"], 1);
        assert_eq!(msg2["events_today"], 1);

        // Same values from direct DB query (midnight/restart invariant).
        let direct = daily_attendance_metrics(&pool, &day).await.unwrap();
        assert_eq!(direct.present_count, 1);
        assert_eq!(direct.events_today, 1);

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[tokio::test]
    async fn emit_skips_on_query_failure_never_zeros() {
        let (mut settings, data_dir, db_path) = temp_settings();
        // Valid connect first, then point pool at a closed/broken scenario via
        // invalid timezone so emit fails before inventing zeros.
        settings.app_timezone = "Not/A_Zone".into();
        // Pool still needs a real DB for connect_pool; create with UTC then swap.
        let mut ok = settings.clone();
        ok.app_timezone = "UTC".into();
        let pool = connect_pool(&ok).await.unwrap();

        let metrics = Arc::new(RwLock::new(VisionMetrics::default()));
        let (tx, mut rx) = broadcast::channel::<WsEvent>(8);
        emit_persisted_metrics(&pool, &settings, &metrics, &tx).await;
        assert!(
            rx.try_recv().is_err(),
            "must not broadcast metrics on timezone/query failure"
        );

        // Recovery: valid timezone emits real zeros for empty day (truthful zero).
        settings.app_timezone = "UTC".into();
        emit_persisted_metrics(&pool, &settings, &metrics, &tx).await;
        let msg = rx.try_recv().expect("recovered metrics");
        assert_eq!(msg["type"], "metrics");
        assert_eq!(msg["present_count"], 0);
        assert_eq!(msg["events_today"], 0);

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn hud_projection_branches() {
        // Approach not eligible → keep approaching (not walkby/cooldown).
        assert_eq!(
            refine_hud_after_identity(
                HudState::Approaching,
                false,
                false,
                IdentityAttempt::NotAttempted
            ),
            HudState::Approaching
        );
        // Actual walk-by.
        assert_eq!(
            refine_hud_after_identity(
                HudState::Tracking,
                true,
                false,
                IdentityAttempt::NotAttempted
            ),
            HudState::Walkby
        );
        // Cooldown only for cooldown skip.
        assert_eq!(
            refine_hud_after_identity(
                HudState::Ready,
                false,
                true,
                IdentityAttempt::Skipped(SkipReason::Cooldown)
            ),
            HudState::Cooldown
        );
        // No transition keeps prior.
        assert_eq!(
            refine_hud_after_identity(
                HudState::Ready,
                false,
                true,
                IdentityAttempt::Skipped(SkipReason::NoTransition)
            ),
            HudState::Ready
        );
        // Commit only after persist.
        assert_eq!(
            refine_hud_after_identity(HudState::Ready, false, true, IdentityAttempt::Committed),
            HudState::Committed
        );
    }
}
