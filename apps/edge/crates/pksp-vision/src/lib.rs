//! Vision: FaceEngine (mock | ort), gallery, capture, worker pipeline.

mod align;
mod scrfd;
mod zones;

pub use align::{align_arcface_bgr, AlignError};
pub use scrfd::{
    decode_scrfd, letterbox_bgr_to_nchw, levels_from_heads, stride_for_score_len, ScrfdError,
    DEFAULT_SCORE_THRESH, NMS_IOU, STRIDES,
};

use pksp_core::{
    assign_tracks, commit_eligible, evaluate_vote, hud_state, l2_normalize, match_top1,
    mean_l2_embedding, pack_embedding, prefer_commit_track, quality_gate_extended, should_vote,
    track_zone, trajectory_is_walkby, Detection, MatchResult, TrackerState, ZoneMap,
};
use pksp_db::{bump_gallery_version, commit_identity, load_gallery_matrix, Settings};
use serde_json::json;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, Semaphore};
use tracing::{info, warn};
pub use zones::load_zones_for_camera;

/// WebSocket / hub event (JSON object).
pub type WsEvent = serde_json::Value;

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

/// Deterministic mock engine — intensity bucket embeddings (parity with Python mock).
pub struct MockFaceEngine {
    dim: usize,
}

impl MockFaceEngine {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    fn vec_for_mean(&self, mean: f32) -> Vec<f32> {
        let bucket = (mean as i32).rem_euclid(50) as u64;
        let mut s = bucket.wrapping_add(7);
        let mut v = Vec::with_capacity(self.dim);
        for _ in 0..self.dim {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
            let f = ((s >> 33) as f32) / (u32::MAX as f32) - 0.5;
            v.push(f);
        }
        let mut s2 = ((mean * 10.0) as i32).rem_euclid(1000) as u64;
        for x in &mut v {
            s2 = s2.wrapping_mul(6364136223846793005).wrapping_add(1);
            let noise = (((s2 >> 33) as f32) / (u32::MAX as f32) - 0.5) * 0.02;
            *x += noise;
        }
        l2_normalize(&v)
    }
}

impl FaceEngine for MockFaceEngine {
    fn ready(&self) -> bool {
        true
    }
    fn model_name(&self) -> &str {
        "mock"
    }
    fn execution_provider(&self) -> &str {
        "mock"
    }
    fn detect_and_embed(
        &self,
        width: u32,
        height: u32,
        bgr: &[u8],
    ) -> Result<Vec<DetectedFace>, FaceError> {
        if width < 20 || height < 20 || bgr.is_empty() {
            return Ok(vec![]);
        }
        let mean = bgr.iter().map(|&x| x as f32).sum::<f32>() / bgr.len() as f32;
        if mean < 5.0 {
            return Ok(vec![]);
        }
        let margin = 0.15f32;
        let x1 = width as f32 * margin;
        let y1 = height as f32 * margin;
        let x2 = width as f32 * (1.0 - margin);
        let y2 = height as f32 * (1.0 - margin);
        // Synthetic landmarks roughly centered
        let cx = (x1 + x2) * 0.5;
        let cy = (y1 + y2) * 0.5;
        let ew = (x2 - x1) * 0.15;
        let landmarks = Some([
            [cx - ew, cy - (y2 - y1) * 0.1],
            [cx + ew, cy - (y2 - y1) * 0.1],
            [cx, cy],
            [cx - ew * 0.8, cy + (y2 - y1) * 0.15],
            [cx + ew * 0.8, cy + (y2 - y1) * 0.15],
        ]);
        Ok(vec![DetectedFace {
            bbox: (x1, y1, x2, y2),
            det_score: 0.99,
            embedding: self.vec_for_mean(mean),
            landmarks,
        }])
    }
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
            // Session construction deferred until ort is linked in a follow-up build.
            // Presence of both weights marks ready; detect_and_embed still needs ort runtime.
            // For now mark not-ready so mock remains default unless we implement ort path.
            // Attempt: if ort feature not compiled, stay unavailable.
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
    // ort sessions are initialized lazily on first detect when feature available.
    // Without the ort crate linked, we report unavailable so MOCK remains safe.
    if std::env::var("PKSP_FORCE_ORT_READY").as_deref() == Ok("1") {
        return Ok(applied_execution_provider(providers));
    }
    // Real ort path: implemented in engine_ort module when dependency present.
    ort_runtime::try_load(model_dir, providers)
}

mod ort_runtime {
    use std::path::Path;
    use std::sync::OnceLock;

    static LOADED: OnceLock<Result<String, String>> = OnceLock::new();

    pub fn try_load(model_dir: &Path, providers: &str) -> Result<String, String> {
        LOADED
            .get_or_init(|| load_inner(model_dir, providers))
            .clone()
    }

    fn load_inner(model_dir: &Path, providers: &str) -> Result<String, String> {
        // When `ort` is available (dependency), this opens sessions.
        // Currently we use a soft probe: files present + optional ort crate.
        #[cfg(feature = "ort")]
        {
            return super::ort_sessions::load(model_dir, providers);
        }
        #[cfg(not(feature = "ort"))]
        {
            let _ = model_dir;
            Err(format!(
                "ort feature not enabled (providers={providers}); build with --features ort"
            ))
        }
    }

    #[allow(dead_code)]
    pub fn clear_for_tests() {
        // OnceLock cannot clear; tests use separate process.
    }
}

#[cfg(feature = "ort")]
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
                    let shape: Vec<i64> = shape.iter().copied().collect();
                    let flat: Vec<f32> = data.iter().copied().collect();
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
                emb_out = Some(data.iter().copied().collect());
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
        "buffalo_l"
    }
    fn execution_provider(&self) -> &str {
        &self.provider
    }
    fn detect_and_embed(&self, w: u32, h: u32, bgr: &[u8]) -> Result<Vec<DetectedFace>, FaceError> {
        if !self.ready {
            return Err(FaceError::NotReady);
        }
        #[cfg(feature = "ort")]
        {
            return ort_sessions::detect_and_embed(w, h, bgr, self.det_size);
        }
        #[cfg(not(feature = "ort"))]
        {
            let _ = (w, h, bgr, self.det_size);
            Err(FaceError::NotReady)
        }
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
    let (ids, names, matrix) = load_gallery_matrix(pool, settings.embedding_dim).await?;
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

#[derive(Default)]
pub struct VisionMetrics {
    pub online: HashMap<String, bool>,
    pub fps: HashMap<String, f64>,
    pub events_today: u64,
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

/// Start vision process loops (synthetic or RTSP depending on settings).
pub fn start_vision_worker(
    pool: SqlitePool,
    settings: Arc<Settings>,
    engine: Arc<dyn FaceEngine>,
    gallery: Arc<RwLock<Gallery>>,
    tx: broadcast::Sender<WsEvent>,
    camera_ids: Vec<String>,
    camera_rtsps: HashMap<String, String>,
) -> VisionHandle {
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
        let rtsp = camera_rtsps.get(&cam_id).cloned().unwrap_or_default();
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

    VisionHandle {
        stop,
        metrics,
        gallery_version,
    }
}

/// Backward-compatible alias.
pub fn start_mock_worker(
    pool: SqlitePool,
    settings: Arc<Settings>,
    engine: Arc<dyn FaceEngine>,
    gallery: Arc<RwLock<Gallery>>,
    tx: broadcast::Sender<WsEvent>,
    camera_ids: Vec<String>,
) -> VisionHandle {
    start_vision_worker(
        pool,
        settings,
        engine,
        gallery,
        tx,
        camera_ids,
        HashMap::new(),
    )
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
    // Seed overwritten on first frame; elapsed used for freeze/offline detection.
    #[allow(unused_assignments)]
    let mut last_frame_ts = Instant::now();
    let mut last_processed_sequence: Option<u64> = None;
    let mut synthetic_sequence: u64 = 0;
    let mut last_infer_err_log = Instant::now() - Duration::from_secs(60);
    let use_rtsp = !settings.mock_vision && !rtsp_url.is_empty() && engine.ready();

    // Shared latest frame for RTSP path
    let latest: LatestFrame = Arc::new(RwLock::new(None));
    if use_rtsp {
        let stop_cap = stop.clone();
        let latest_c = latest.clone();
        let ffmpeg = settings.ffmpeg_bin.clone();
        let url = rtsp_url.clone();
        let cam = camera_id.clone();
        tokio::task::spawn_blocking(move || {
            capture_ffmpeg_loop(&cam, &url, &ffmpeg, stop_cap, latest_c);
        });
    }

    {
        let mut m = metrics.write().unwrap();
        m.online.insert(camera_id.clone(), true);
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

        let (w, h, bgr, sequence) = if use_rtsp {
            let snap = latest.read().unwrap().clone();
            match snap {
                Some(frame) => {
                    last_frame_ts = frame.captured_at;
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
                    // Own the snapshot before marking processed.
                    last_processed_sequence = Some(frame.sequence);
                    (frame.width, frame.height, frame.bgr, frame.sequence)
                }
                None => {
                    metrics
                        .write()
                        .unwrap()
                        .online
                        .insert(camera_id.clone(), false);
                    continue;
                }
            }
        } else {
            // Synthetic BGR frame — each tick is a new observation.
            let w = 320u32;
            let h = 320u32;
            let phase = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            let intensity = (40 + ((phase as i32).wrapping_mul(17)).rem_euclid(180)) as u8;
            let mut bgr = vec![intensity.saturating_sub(15); (w * h * 3) as usize];
            for y in (h / 4)..(3 * h / 4) {
                for x in (w / 4)..(3 * w / 4) {
                    let i = ((y * w + x) * 3) as usize;
                    bgr[i] = intensity;
                    bgr[i + 1] = intensity.saturating_add(10);
                    bgr[i + 2] = intensity.saturating_add(5);
                }
            }
            synthetic_sequence = synthetic_sequence.wrapping_add(1);
            last_frame_ts = Instant::now();
            metrics
                .write()
                .unwrap()
                .online
                .insert(camera_id.clone(), true);
            last_processed_sequence = Some(synthetic_sequence);
            (w, h, bgr, synthetic_sequence)
        };
        let _ = sequence;

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
            &metrics,
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
        let age_ms = last_frame_ts.elapsed().as_millis() as u64;
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
            let m = metrics.read().unwrap();
            let online_n = m.online.values().filter(|v| **v).count();
            let _ = tx.send(json!({
                "type": "metrics",
                "cameras_online": online_n,
                "present_count": 0,
                "events_today": m.events_today,
                "vision_fps": m.fps,
            }));
        }
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
    metrics: &Arc<RwLock<VisionMetrics>>,
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
        let mut state = hud_state(
            tr.quality_ok || tr.label == "LOW QUALITY",
            tr.employee_id,
            zone,
            is_walkby,
            smart,
            zones_empty,
        )
        .as_str()
        .to_string();
        if !tr.quality_ok && tr.label == "LOW QUALITY" {
            state = "low_quality".into();
        }

        let can_try_commit = tr.quality_ok
            && tr.employee_id.is_some()
            && preferred_id.map(|id| id == tr.track_id).unwrap_or(true);

        if can_try_commit {
            if let Some(commit) = evaluate_vote(
                tr,
                settings.vote_window,
                settings.vote_min_hits,
                settings.match_threshold,
            ) {
                let eligible = commit_eligible(
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
                        Ok(Some((event_id, name, kind))) => {
                            state = "committed".into();
                            // mark last_commit_ts on live tracker
                            if let Some(t) = tracker
                                .tracks
                                .iter_mut()
                                .find(|t| t.track_id == tr.track_id)
                            {
                                t.last_commit_ts = Some(ts);
                                t.state = "committed".into();
                            }
                            {
                                let mut m = metrics.write().unwrap();
                                m.events_today += 1;
                            }
                            let ts = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs_f64();
                            let _ = tx.send(json!({
                                "type": "attendance",
                                "event_id": event_id,
                                "employee_id": commit.employee_id,
                                "name": name,
                                "kind": kind,
                                "camera_id": camera_id,
                                "score": commit.avg_score,
                                "ts": ts,
                            }));
                        }
                        Ok(None) => {
                            state = "cooldown".into();
                        }
                        Err(e) => warn!("commit failed: {e}"),
                    }
                } else {
                    state = "walkby".into();
                }
            }
        }
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

/// Crop bbox to grayscale luma for blur/exposure quality extensions.
fn crop_gray_luma(
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

/// Enroll images using FaceEngine (mock-safe).
pub async fn enroll_images(
    pool: &SqlitePool,
    settings: &Settings,
    engine: &dyn FaceEngine,
    employee_id: i64,
    files: Vec<(String, Vec<u8>)>,
) -> anyhow::Result<serde_json::Value> {
    let mut rejected = Vec::new();
    let mut usable = 0usize;
    let mut vectors = Vec::new();
    let dest = settings.enroll_dir().join(employee_id.to_string());
    std::fs::create_dir_all(&dest)?;

    for (filename, data) in &files {
        let img = image::load_from_memory(data);
        let (emb, reason) = match img {
            Ok(im) => {
                let rgb = im.to_rgb8();
                let (w, h) = rgb.dimensions();
                let mut bgr = Vec::with_capacity((w * h * 3) as usize);
                for p in rgb.pixels() {
                    bgr.push(p[2]);
                    bgr.push(p[1]);
                    bgr.push(p[0]);
                }
                let faces = match engine.detect_and_embed(w, h, &bgr) {
                    Ok(f) => f,
                    Err(e) => {
                        return Ok(json!({
                            "ok": false,
                            "error": e.to_string(),
                            "usable": 0,
                            "rejected": [],
                        }));
                    }
                };
                if faces.is_empty() {
                    (None, Some("no_face".to_string()))
                } else {
                    let mut faces = faces;
                    faces.sort_by(|a, b| {
                        let aa = (a.bbox.2 - a.bbox.0) * (a.bbox.3 - a.bbox.1);
                        let bb = (b.bbox.2 - b.bbox.0) * (b.bbox.3 - b.bbox.1);
                        bb.partial_cmp(&aa).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let face = &faces[0];
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
                    if !q.ok {
                        (None, q.reason)
                    } else {
                        (Some(face.embedding.clone()), None)
                    }
                }
            }
            Err(_) => (None, Some("decode_error".to_string())),
        };

        let uid = uuid::Uuid::new_v4().simple().to_string();
        let uid = &uid[..12.min(uid.len())];
        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg");
        let rel = format!("enroll/{employee_id}/{uid}.{ext}");
        let abs = settings.data_dir.join(&rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&abs, data)?;

        let ok = emb.is_some();
        pksp_db::add_employee_image(pool, employee_id, &rel, ok, reason.as_deref()).await?;
        if let Some(v) = emb {
            vectors.push(v);
            usable += 1;
        } else {
            rejected.push(json!({
                "filename": filename,
                "reason": reason.unwrap_or_else(|| "no_face".into())
            }));
        }
    }

    let embedding_ready = if vectors.len() >= settings.min_enroll_images {
        let mean = mean_l2_embedding(&vectors, settings.embedding_dim)?;
        let blob = pack_embedding(&mean, settings.embedding_dim)?;
        pksp_db::save_embedding(
            pool,
            employee_id,
            &blob,
            settings.embedding_dim,
            vectors.len() as i32,
            engine.model_name(),
        )
        .await?;
        true
    } else {
        false
    };

    let _ = bump_gallery_version(pool).await?;
    info!(
        "enroll employee={employee_id} received={} usable={usable} ready={embedding_ready}",
        files.len()
    );

    Ok(json!({
        "received": files.len(),
        "usable": usable,
        "rejected": rejected,
        "embedding_ready": embedding_ready,
        "num_images_used": if embedding_ready { vectors.len() } else { 0 },
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
            "mock"
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
            "mock"
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
            "mock"
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
            "mock"
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
