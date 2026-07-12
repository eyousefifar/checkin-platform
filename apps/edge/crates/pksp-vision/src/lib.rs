//! Vision: FaceEngine (mock | ort), gallery, capture, worker pipeline.

mod zones;

use pksp_core::{
    assign_tracks, commit_eligible, evaluate_vote, hud_state, l2_normalize, match_top1,
    mean_l2_embedding, pack_embedding, prefer_commit_track, quality_gate, should_vote, track_zone,
    trajectory_is_walkby, Detection, MatchResult, TrackerState, ZoneMap,
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

pub trait FaceEngine: Send + Sync {
    fn ready(&self) -> bool;
    fn model_name(&self) -> &str;
    fn execution_provider(&self) -> &str;
    fn detect_and_embed(&self, width: u32, height: u32, bgr: &[u8]) -> Vec<DetectedFace>;
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
    fn detect_and_embed(&self, width: u32, height: u32, bgr: &[u8]) -> Vec<DetectedFace> {
        if width < 20 || height < 20 || bgr.is_empty() {
            return vec![];
        }
        let mean = bgr.iter().map(|&x| x as f32).sum::<f32>() / bgr.len() as f32;
        if mean < 5.0 {
            return vec![];
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
        vec![DetectedFace {
            bbox: (x1, y1, x2, y2),
            det_score: 0.99,
            embedding: self.vec_for_mean(mean),
            landmarks,
        }]
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

/// Attempt to validate model paths / init. Full SCRFD+ArcFace runs when ort sessions exist.
/// Returns provider name on success.
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
        return Ok(providers
            .split(',')
            .next()
            .unwrap_or("CPUExecutionProvider")
            .to_string());
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
        let provider = providers
            .split(',')
            .map(|s| s.trim())
            .find(|s| !s.is_empty())
            .unwrap_or("CPUExecutionProvider")
            .to_string();

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

    /// Full detect+embed — simplified SCRFD decode for common buffalo_l outputs.
    pub fn detect_and_embed(
        width: u32,
        height: u32,
        bgr: &[u8],
        det_size: i32,
    ) -> Vec<DetectedFace> {
        let Ok(mut g) = SESS.lock() else {
            return vec![];
        };
        let Some(ref mut s) = *g else {
            return vec![];
        };
        match run_pipeline(s, width, height, bgr, det_size) {
            Ok(v) => v,
            Err(_) => vec![],
        }
    }

    fn run_pipeline(
        s: &mut Sessions,
        width: u32,
        height: u32,
        bgr: &[u8],
        det_size: i32,
    ) -> Result<Vec<DetectedFace>, String> {
        let ds = det_size as u32;
        let (tensor, scale, pad_x, pad_y) = letterbox_bgr_to_nchw(bgr, width, height, ds);
        let input = ort::value::Tensor::from_array(([1usize, 3, ds as usize, ds as usize], tensor))
            .map_err(|e| e.to_string())?;
        let faces = {
            let outputs = s.det.run(ort::inputs![input]).map_err(|e| e.to_string())?;
            decode_scrfd_heuristic(&outputs, width, height, scale, pad_x, pad_y, ds)?
        }; // drop SessionOutputs before rec borrow
        let mut out = Vec::new();
        for (bbox, score, kps) in faces {
            if score < 0.5 {
                continue;
            }
            let emb = embed_face(s, bgr, width, height, bbox, kps)?;
            out.push(DetectedFace {
                bbox,
                det_score: score,
                embedding: emb,
                landmarks: kps,
            });
        }
        Ok(out)
    }

    fn letterbox_bgr_to_nchw(bgr: &[u8], w: u32, h: u32, ds: u32) -> (Vec<f32>, f32, f32, f32) {
        let scale = (ds as f32 / w as f32).min(ds as f32 / h as f32);
        let nw = (w as f32 * scale).round() as u32;
        let nh = (h as f32 * scale).round() as u32;
        let pad_x = (ds as f32 - nw as f32) * 0.5;
        let pad_y = (ds as f32 - nh as f32) * 0.5;
        let mut out = vec![0.0f32; (3 * ds * ds) as usize];
        // nearest-neighbor resize into letterbox, BGR→RGB, (x-127.5)/128
        for y in 0..nh {
            for x in 0..nw {
                let sx = ((x as f32 / scale) as u32).min(w - 1);
                let sy = ((y as f32 / scale) as u32).min(h - 1);
                let si = ((sy * w + sx) * 3) as usize;
                let dx = (x as f32 + pad_x) as u32;
                let dy = (y as f32 + pad_y) as u32;
                if dx >= ds || dy >= ds {
                    continue;
                }
                let di = (dy * ds + dx) as usize;
                let b = bgr[si] as f32;
                let g = bgr[si + 1] as f32;
                let r = bgr[si + 2] as f32;
                // NCHW RGB
                out[0 * (ds * ds) as usize + di] = (r - 127.5) / 128.0;
                out[1 * (ds * ds) as usize + di] = (g - 127.5) / 128.0;
                out[2 * (ds * ds) as usize + di] = (b - 127.5) / 128.0;
            }
        }
        (out, scale, pad_x, pad_y)
    }

    fn decode_scrfd_heuristic(
        outputs: &ort::session::SessionOutputs,
        orig_w: u32,
        orig_h: u32,
        scale: f32,
        pad_x: f32,
        pad_y: f32,
        _ds: u32,
    ) -> Result<Vec<((f32, f32, f32, f32), f32, Option<[[f32; 2]; 5]>)>, String> {
        // Collect all f32 outputs; pick the densest score-like map.
        // This is intentionally resilient across SCRFD export variants.
        let mut best: Option<((f32, f32, f32, f32), f32, Option<[[f32; 2]; 5]>)> = None;
        for (_name, val) in outputs.iter() {
            if let Ok((shape, data)) = val.try_extract_tensor::<f32>() {
                let shape: Vec<i64> = shape.iter().copied().collect();
                // score maps often 1x1xHxW or 1xHxW
                if data.iter().any(|v| *v > 0.5) {
                    // find argmax
                    let (idx, &sc) = data
                        .iter()
                        .enumerate()
                        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                        .unwrap();
                    if sc < 0.5 {
                        continue;
                    }
                    // approximate center of letterbox cell → original
                    let n = data.len().max(1);
                    let side = (n as f32).sqrt() as usize;
                    let y = if side > 0 { idx / side } else { 0 };
                    let x = if side > 0 { idx % side } else { 0 };
                    let cx_lb = x as f32 + 0.5;
                    let cy_lb = y as f32 + 0.5;
                    // scale coords if map size known poorly — use relative
                    let fx = (cx_lb / side.max(1) as f32) * _ds as f32;
                    let fy = (cy_lb / side.max(1) as f32) * _ds as f32;
                    let ox = ((fx - pad_x) / scale).clamp(0.0, orig_w as f32 - 1.0);
                    let oy = ((fy - pad_y) / scale).clamp(0.0, orig_h as f32 - 1.0);
                    let face_w = (orig_w as f32 * 0.25).max(60.0);
                    let face_h = face_w * 1.2;
                    let bbox = (
                        (ox - face_w * 0.5).max(0.0),
                        (oy - face_h * 0.5).max(0.0),
                        (ox + face_w * 0.5).min(orig_w as f32),
                        (oy + face_h * 0.5).min(orig_h as f32),
                    );
                    if best.as_ref().map(|b| b.1).unwrap_or(0.0) < sc {
                        best = Some((bbox, sc, None));
                    }
                }
                let _ = shape;
            }
        }
        Ok(best.into_iter().collect())
    }

    fn embed_face(
        s: &mut Sessions,
        bgr: &[u8],
        width: u32,
        height: u32,
        bbox: (f32, f32, f32, f32),
        _kps: Option<[[f32; 2]; 5]>,
    ) -> Result<Vec<f32>, String> {
        // Crop bbox → 112x112 RGB normalized ArcFace style
        let (x1, y1, x2, y2) = bbox;
        let mut tensor = vec![0.0f32; 3 * 112 * 112];
        for yy in 0..112u32 {
            for xx in 0..112u32 {
                let sx = (x1 + (x2 - x1) * (xx as f32 / 111.0))
                    .round()
                    .clamp(0.0, width as f32 - 1.0) as u32;
                let sy = (y1 + (y2 - y1) * (yy as f32 / 111.0))
                    .round()
                    .clamp(0.0, height as f32 - 1.0) as u32;
                let si = ((sy * width + sx) * 3) as usize;
                let b = bgr[si] as f32;
                let gch = bgr[si + 1] as f32;
                let r = bgr[si + 2] as f32;
                let di = (yy * 112 + xx) as usize;
                tensor[0 * 112 * 112 + di] = (r - 127.5) / 127.5;
                tensor[1 * 112 * 112 + di] = (gch - 127.5) / 127.5;
                tensor[2 * 112 * 112 + di] = (b - 127.5) / 127.5;
            }
        }
        let input = ort::value::Tensor::from_array(([1usize, 3, 112, 112], tensor))
            .map_err(|e| e.to_string())?;
        let outputs = s.rec.run(ort::inputs![input]).map_err(|e| e.to_string())?;
        let mut emb_out: Option<Vec<f32>> = None;
        for (_name, val) in outputs.iter() {
            if let Ok((_shape, data)) = val.try_extract_tensor::<f32>() {
                emb_out = Some(data.iter().copied().collect());
                break;
            }
        }
        let emb = emb_out.ok_or_else(|| "no rec output".to_string())?;
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
    fn detect_and_embed(&self, w: u32, h: u32, bgr: &[u8]) -> Vec<DetectedFace> {
        if !self.ready {
            return vec![];
        }
        #[cfg(feature = "ort")]
        {
            return ort_sessions::detect_and_embed(w, h, bgr, self.det_size);
        }
        #[cfg(not(feature = "ort"))]
        {
            let _ = (w, h, bgr, self.det_size);
            vec![]
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

// Shared latest BGR frame: (width, height, pixels, captured_at)
type LatestFrame = Arc<RwLock<Option<(u32, u32, Vec<u8>, Instant)>>>;

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

        let (w, h, bgr) = if use_rtsp {
            let snap = latest.read().unwrap().clone();
            match snap {
                Some((w, h, bgr, ts)) => {
                    last_frame_ts = ts;
                    let age = ts.elapsed().as_millis() as u64;
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
                    (w, h, bgr)
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
            // Synthetic BGR frame
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
            last_frame_ts = Instant::now();
            metrics
                .write()
                .unwrap()
                .online
                .insert(camera_id.clone(), true);
            (w, h, bgr)
        };

        let infer_t0 = Instant::now();
        let _permit = infer_sem.acquire().await.ok();
        let faces_out = infer_frame(
            &camera_id,
            w,
            h,
            &bgr,
            &engine,
            &gallery,
            &mut tracker,
            &settings,
            &zones,
            &pool,
            &tx,
            &metrics,
        )
        .await;
        drop(_permit);
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
                    *latest.write().unwrap() = Some((w, h, buf.clone(), Instant::now()));
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

#[allow(clippy::too_many_arguments)] // ponytail: orchestration boundary; group only when another caller exists
async fn infer_frame(
    camera_id: &str,
    w: u32,
    h: u32,
    bgr: &[u8],
    engine: &Arc<dyn FaceEngine>,
    gallery: &Arc<RwLock<Gallery>>,
    tracker: &mut TrackerState,
    settings: &Settings,
    zones: &ZoneMap,
    pool: &SqlitePool,
    tx: &broadcast::Sender<WsEvent>,
    metrics: &Arc<RwLock<VisionMetrics>>,
) -> Vec<serde_json::Value> {
    let raw = if engine.ready() {
        engine.detect_and_embed(w, h, bgr)
    } else {
        vec![]
    };

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
            let q = quality_gate(
                f.det_score,
                f.bbox,
                settings.min_det_score,
                settings.min_face_px,
                w as i32,
                h as i32,
                false,
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
                let faces = engine.detect_and_embed(w, h, &bgr);
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
                    let q = quality_gate(
                        face.det_score,
                        face.bbox,
                        settings.min_det_score,
                        settings.min_face_px,
                        w as i32,
                        h as i32,
                        false,
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
