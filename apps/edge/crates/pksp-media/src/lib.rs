//! Media plane: supervise **bundled** MediaMTX (+ optional ffmpeg publisher).
//!
//! Binaries live under `apps/edge/bin/` (see `scripts/download-binaries.sh`).
//! Source RTSP URLs are never written to application logs, API status, or child
//! error text. FFmpeg still receives the input URL in process arguments — that
//! is an accepted boundary only under a dedicated service account on a dedicated
//! appliance.

use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::{Method, Request, StatusCode};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// How the browser-safe path is fed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaSourceMode {
    /// MediaMTX only; an external process (demo, operator) publishes.
    External,
    /// Stream-copy H.264 from `CAM_IN_H264_RTSP` into the publish path.
    Copy,
    /// Transcode `CAM_IN_RTSP` to H.264 and publish.
    Transcode,
}

impl MediaSourceMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "external" => Ok(Self::External),
            "copy" => Ok(Self::Copy),
            "transcode" => Ok(Self::Transcode),
            other => Err(format!(
                "unknown MEDIA_SOURCE_MODE '{other}' (expected external|copy|transcode)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::External => "external",
            Self::Copy => "copy",
            Self::Transcode => "transcode",
        }
    }
}

/// Publication readiness for the browser-safe path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PublicationState {
    #[default]
    Unavailable,
    Starting,
    Ready,
}

impl PublicationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Starting => "starting",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaConfig {
    /// Path or name; empty / "mediamtx" → auto-resolve bundled bin.
    pub mediamtx_bin: String,
    pub config_path: PathBuf,
    /// ffmpeg for copy/transcode (empty → auto-resolve).
    pub ffmpeg_bin: String,
    /// Source URL for copy/transcode. Never log or expose.
    pub source_rtsp: Option<String>,
    pub source_mode: MediaSourceMode,
    /// Publisher path name inside MediaMTX (e.g. `cam_in_h264`).
    pub publish_path: String,
    /// Working directory for child processes (logs, relative config).
    pub work_dir: PathBuf,
    /// Loopback MediaMTX HTTP API address.
    pub mediamtx_api_addr: SocketAddr,
}

#[derive(Debug, Default, Clone)]
pub struct MediaStatus {
    pub mediamtx_running: bool,
    /// True while the supervised ffmpeg publisher child is alive (copy/transcode).
    pub transcoder_running: bool,
    pub publication: PublicationState,
    pub last_error: Option<String>,
    /// Set only when the API reports the publish path ready with a live publisher.
    pub preferred_webrtc_path: Option<String>,
    pub mediamtx_path: Option<String>,
    pub ffmpeg_path: Option<String>,
    pub source_mode: Option<String>,
}

/// Validated publication policy (no credential-bearing fields in Display).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMediaPolicy {
    pub mode: MediaSourceMode,
    pub publish_path: String,
    /// Whether ffmpeg will be supervised.
    pub needs_ffmpeg: bool,
}

/// Parse and validate source mode + inputs. Does not retain source URLs.
pub fn resolve_media_policy(
    mode: MediaSourceMode,
    cam_in_rtsp: &str,
    cam_in_h264_rtsp: &str,
    publish_path: &str,
) -> Result<ResolvedMediaPolicy, String> {
    validate_path_name(publish_path)?;
    match mode {
        MediaSourceMode::External => {
            if !cam_in_h264_rtsp.is_empty() {
                return Err(
                    "MEDIA_SOURCE_MODE=external rejects non-empty CAM_IN_H264_RTSP \
                     (use copy mode for native H.264 publication)"
                        .into(),
                );
            }
            Ok(ResolvedMediaPolicy {
                mode,
                publish_path: publish_path.to_string(),
                needs_ffmpeg: false,
            })
        }
        MediaSourceMode::Copy => {
            if cam_in_h264_rtsp.is_empty() {
                return Err("MEDIA_SOURCE_MODE=copy requires non-empty CAM_IN_H264_RTSP".into());
            }
            Ok(ResolvedMediaPolicy {
                mode,
                publish_path: publish_path.to_string(),
                needs_ffmpeg: true,
            })
        }
        MediaSourceMode::Transcode => {
            if cam_in_rtsp.is_empty() {
                return Err("MEDIA_SOURCE_MODE=transcode requires non-empty CAM_IN_RTSP".into());
            }
            Ok(ResolvedMediaPolicy {
                mode,
                publish_path: publish_path.to_string(),
                needs_ffmpeg: true,
            })
        }
    }
}

/// Build MediaConfig from mode + settings fields. Returns Err on invalid combo.
#[allow(clippy::too_many_arguments)] // thin factory over validated settings fields
pub fn build_media_config(
    mode: MediaSourceMode,
    cam_in_rtsp: &str,
    cam_in_h264_rtsp: &str,
    publish_path: &str,
    mediamtx_bin: String,
    config_path: PathBuf,
    ffmpeg_bin: String,
    work_dir: PathBuf,
    mediamtx_api_addr: SocketAddr,
) -> Result<MediaConfig, String> {
    let policy = resolve_media_policy(mode, cam_in_rtsp, cam_in_h264_rtsp, publish_path)?;
    validate_loopback_api(mediamtx_api_addr)?;
    let source_rtsp = match mode {
        MediaSourceMode::External => None,
        MediaSourceMode::Copy => Some(cam_in_h264_rtsp.to_string()),
        MediaSourceMode::Transcode => Some(cam_in_rtsp.to_string()),
    };
    let _ = policy;
    Ok(MediaConfig {
        mediamtx_bin,
        config_path,
        ffmpeg_bin,
        source_rtsp,
        source_mode: mode,
        publish_path: publish_path.to_string(),
        work_dir,
        mediamtx_api_addr,
    })
}

pub fn validate_path_name(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("publish path must not be empty".into());
    }
    if !path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("publish path '{path}' must match [A-Za-z0-9_-]+"));
    }
    Ok(())
}

pub fn validate_loopback_api(addr: SocketAddr) -> Result<(), String> {
    if !addr.ip().is_loopback() {
        return Err(format!(
            "MEDIAMTX_API_ADDR must be loopback, got {}",
            addr.ip()
        ));
    }
    Ok(())
}

pub fn parse_mediamtx_api_addr(s: &str) -> Result<SocketAddr, String> {
    let addr: SocketAddr = s
        .parse()
        .map_err(|e| format!("malformed MEDIAMTX_API_ADDR '{s}': {e}"))?;
    validate_loopback_api(addr)?;
    Ok(addr)
}

/// FFmpeg argv after the binary, with a placeholder for the input URL.
/// Tests compare this projection — never a credential-bearing fixture.
pub fn ffmpeg_arg_projection(mode: MediaSourceMode, publish_path: &str) -> Vec<String> {
    let rtmp = format!("rtmp://127.0.0.1:1935/{publish_path}");
    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-rtsp_transport".into(),
        "tcp".into(),
        "-i".into(),
        "<redacted>".into(),
        "-an".into(),
    ];
    match mode {
        MediaSourceMode::Copy => {
            args.extend(["-c:v".into(), "copy".into()]);
        }
        MediaSourceMode::Transcode => {
            args.extend([
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "ultrafast".into(),
                "-tune".into(),
                "zerolatency".into(),
                "-b:v".into(),
                "1800k".into(),
            ]);
        }
        MediaSourceMode::External => {}
    }
    args.extend(["-f".into(), "flv".into(), rtmp]);
    args
}

fn ffmpeg_spawn_args(mode: MediaSourceMode, source_rtsp: &str, publish_path: &str) -> Vec<String> {
    let mut args = ffmpeg_arg_projection(mode, publish_path);
    // Replace redaction placeholder with real input for the child only.
    if let Some(slot) = args.iter_mut().find(|a| *a == "<redacted>") {
        *slot = source_rtsp.to_string();
    }
    args
}

pub struct MediaSupervisor {
    cfg: MediaConfig,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<MediaStatus>>,
    child: Arc<Mutex<Option<Child>>>,
    ffmpeg_child: Arc<Mutex<Option<Child>>>,
}

impl MediaSupervisor {
    pub fn new(cfg: MediaConfig) -> Self {
        let source_mode = Some(cfg.source_mode.as_str().into());
        Self {
            cfg,
            stop: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(MediaStatus {
                source_mode,
                publication: PublicationState::Unavailable,
                ..MediaStatus::default()
            })),
            child: Arc::new(Mutex::new(None)),
            ffmpeg_child: Arc::new(Mutex::new(None)),
        }
    }

    pub fn status_handle(&self) -> Arc<Mutex<MediaStatus>> {
        self.status.clone()
    }

    pub fn start(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            this.run_mediamtx_loop().await;
        });
        if self.cfg.source_mode != MediaSourceMode::External && self.cfg.source_rtsp.is_some() {
            let this = self.clone();
            tokio::spawn(async move {
                this.run_publisher_loop().await;
            });
        }
        let this = self.clone();
        tokio::spawn(async move {
            this.run_readiness_loop().await;
        });
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    async fn clear_ready(&self, reason: Option<String>) {
        let mut st = self.status.lock().await;
        st.preferred_webrtc_path = None;
        if st.mediamtx_running {
            st.publication = PublicationState::Starting;
        } else {
            st.publication = PublicationState::Unavailable;
        }
        if let Some(r) = reason {
            st.last_error = Some(r);
        }
    }

    async fn run_mediamtx_loop(&self) {
        while !self.stop.load(Ordering::SeqCst) {
            let Some(bin) = resolve_bin(&self.cfg.mediamtx_bin, "mediamtx") else {
                let msg = "mediamtx not found (looked for bundled apps/edge/bin/mediamtx, PATH). Run apps/edge/scripts/download-binaries.sh".to_string();
                warn!("{msg}");
                let mut st = self.status.lock().await;
                st.mediamtx_running = false;
                st.publication = PublicationState::Unavailable;
                st.preferred_webrtc_path = None;
                st.last_error = Some(msg);
                tokio::time::sleep(Duration::from_secs(15)).await;
                continue;
            };

            let cfg_path = ensure_config(&self.cfg.config_path, &self.cfg.work_dir);
            {
                let mut st = self.status.lock().await;
                st.mediamtx_path = Some(bin.display().to_string());
            }

            info!(
                "starting bundled MediaMTX bin={} config={}",
                bin.display(),
                cfg_path.display()
            );

            let mut cmd = Command::new(&bin);
            cmd.arg(cfg_path.as_os_str());
            cmd.current_dir(&self.cfg.work_dir);
            cmd.stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);

            match cmd.spawn() {
                Ok(child) => {
                    {
                        let mut st = self.status.lock().await;
                        st.mediamtx_running = true;
                        st.publication = PublicationState::Starting;
                        st.last_error = None;
                    }
                    *self.child.lock().await = Some(child);
                    if let Some(mut c) = self.child.lock().await.take() {
                        let _ = c.wait().await;
                    }
                    self.clear_ready(Some("MediaMTX exited".into())).await;
                    self.status.lock().await.mediamtx_running = false;
                    if self.stop.load(Ordering::SeqCst) {
                        break;
                    }
                    warn!("MediaMTX exited; restarting in 2s");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                Err(e) => {
                    // Do not include config path secrets; binary path is fine.
                    let msg = format!("failed to spawn mediamtx: {e}");
                    warn!("{msg}");
                    let mut st = self.status.lock().await;
                    st.last_error = Some(msg);
                    st.mediamtx_running = false;
                    st.publication = PublicationState::Unavailable;
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn run_publisher_loop(&self) {
        let Some(rtsp) = self.cfg.source_rtsp.clone() else {
            return;
        };
        let mode = self.cfg.source_mode;
        let path = self.cfg.publish_path.clone();

        // Wait briefly for MediaMTX RTMP to come up
        tokio::time::sleep(Duration::from_secs(2)).await;

        while !self.stop.load(Ordering::SeqCst) {
            let Some(ff) = resolve_bin(&self.cfg.ffmpeg_bin, "ffmpeg") else {
                let msg =
                    "ffmpeg not found in apps/edge/bin or PATH; publisher disabled".to_string();
                warn!("{msg}");
                let mut st = self.status.lock().await;
                st.transcoder_running = false;
                st.last_error = Some(msg);
                self.clear_ready(None).await;
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            };
            {
                let mut st = self.status.lock().await;
                st.ffmpeg_path = Some(ff.display().to_string());
            }

            info!(
                mode = mode.as_str(),
                path = %path,
                "starting supervised ffmpeg publisher"
            );

            let args = ffmpeg_spawn_args(mode, &rtsp, &path);
            let mut cmd = Command::new(&ff);
            cmd.args(&args);
            cmd.stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);

            match cmd.spawn() {
                Ok(child) => {
                    {
                        let mut st = self.status.lock().await;
                        st.transcoder_running = true;
                        // Child spawn alone never marks ready — readiness loop does.
                        if st.publication != PublicationState::Ready {
                            st.publication = PublicationState::Starting;
                        }
                    }
                    *self.ffmpeg_child.lock().await = Some(child);
                    if let Some(mut c) = self.ffmpeg_child.lock().await.take() {
                        let _ = c.wait().await;
                    }
                    {
                        let mut st = self.status.lock().await;
                        st.transcoder_running = false;
                        st.preferred_webrtc_path = None;
                        if st.mediamtx_running {
                            st.publication = PublicationState::Starting;
                        } else {
                            st.publication = PublicationState::Unavailable;
                        }
                    }
                    if self.stop.load(Ordering::SeqCst) {
                        break;
                    }
                    warn!("ffmpeg publisher exited; restarting in 5s");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(e) => {
                    // Never include the RTSP URL from args.
                    warn!("ffmpeg spawn failed: {e}");
                    self.status.lock().await.last_error = Some(format!("ffmpeg spawn failed: {e}"));
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        }
    }

    async fn run_readiness_loop(&self) {
        let mut delay = Duration::from_millis(250);
        while !self.stop.load(Ordering::SeqCst) {
            let mediamtx_up = self.status.lock().await.mediamtx_running;
            if !mediamtx_up {
                self.clear_ready(None).await;
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }

            match path_ready(&self.cfg.mediamtx_api_addr, &self.cfg.publish_path).await {
                Ok(true) => {
                    let mut st = self.status.lock().await;
                    st.publication = PublicationState::Ready;
                    st.preferred_webrtc_path = Some(self.cfg.publish_path.clone());
                    st.last_error = None;
                    delay = Duration::from_millis(250);
                }
                Ok(false) => {
                    let mut st = self.status.lock().await;
                    st.preferred_webrtc_path = None;
                    st.publication = if st.mediamtx_running {
                        PublicationState::Starting
                    } else {
                        PublicationState::Unavailable
                    };
                    delay = (delay * 2).min(Duration::from_secs(2));
                }
                Err(_) => {
                    let mut st = self.status.lock().await;
                    st.preferred_webrtc_path = None;
                    st.publication = if st.mediamtx_running {
                        PublicationState::Starting
                    } else {
                        PublicationState::Unavailable
                    };
                    delay = (delay * 2).min(Duration::from_secs(2));
                }
            }
            tokio::time::sleep(delay).await;
        }
    }
}

#[derive(Debug, Deserialize)]
struct PathGetResponse {
    name: Option<String>,
    ready: Option<bool>,
    source: Option<serde_json::Value>,
}

/// Poll MediaMTX API for a ready path with a live publisher.
pub async fn path_ready(api: &SocketAddr, path: &str) -> Result<bool, String> {
    validate_path_name(path)?;
    validate_loopback_api(*api)?;
    let uri = format!("http://{api}/v3/paths/get/{path}");
    let client = Client::builder(TokioExecutor::new()).build(HttpConnector::new());
    let req = Request::builder()
        .method(Method::GET)
        .uri(&uri)
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .map_err(|e| e.to_string())?;
    let res = tokio::time::timeout(Duration::from_secs(2), client.request(req))
        .await
        .map_err(|_| "mediamtx api timeout".to_string())?
        .map_err(|e| format!("mediamtx api request failed: {e}"))?;
    if res.status() != StatusCode::OK {
        return Ok(false);
    }
    let body = response_json(res).await?;
    let parsed: PathGetResponse =
        serde_json::from_value(body).map_err(|e| format!("malformed path response: {e}"))?;
    if let Some(name) = &parsed.name {
        if name != path {
            return Ok(false);
        }
    }
    let ready = parsed.ready.unwrap_or(false);
    let has_source = parsed
        .source
        .as_ref()
        .map(|s| !s.is_null())
        .unwrap_or(false);
    Ok(ready && has_source)
}

async fn response_json(res: hyper::Response<Incoming>) -> Result<serde_json::Value, String> {
    let bytes = res
        .into_body()
        .collect()
        .await
        .map_err(|e| e.to_string())?
        .to_bytes();
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid json: {e}"))
}

/// Resolve a tool name against env path, bundled `apps/edge/bin`, exe dir, then PATH.
pub fn resolve_bin(configured: &str, default_name: &str) -> Option<PathBuf> {
    let candidates: Vec<String> = if configured.is_empty() || configured == default_name {
        vec![default_name.to_string()]
    } else {
        vec![configured.to_string(), default_name.to_string()]
    };

    for name in candidates {
        let p = PathBuf::from(&name);
        if p.is_file() {
            return Some(p);
        }

        let edge_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../bin")
            .join(&name);
        if let Ok(canon) = edge_bin.canonicalize() {
            if canon.is_file() {
                return Some(canon);
            }
        } else if edge_bin.is_file() {
            return Some(edge_bin);
        }

        for rel in [
            PathBuf::from("bin").join(&name),
            PathBuf::from("apps/edge/bin").join(&name),
        ] {
            if rel.is_file() {
                return Some(rel);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let cand = dir.join(&name);
                if cand.is_file() {
                    return Some(cand);
                }
                let up = dir.join("../../../bin").join(&name);
                if up.is_file() {
                    return Some(up);
                }
            }
        }

        if let Some(found) = which_bin(&name) {
            return Some(found);
        }
    }
    None
}

fn which_bin(name: &str) -> Option<PathBuf> {
    if name.contains('/') || name.contains('\\') {
        let p = Path::new(name);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn ensure_config(configured: &Path, work_dir: &Path) -> PathBuf {
    if configured.is_file() {
        return configured.to_path_buf();
    }
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../configs/mediamtx.yml");
    if bundled.is_file() {
        return bundled;
    }
    let cwd = PathBuf::from("apps/edge/configs/mediamtx.yml");
    if cwd.is_file() {
        return cwd;
    }
    let local = PathBuf::from("configs/mediamtx.yml");
    if local.is_file() {
        return local;
    }
    let out = work_dir.join("mediamtx.yml");
    if !out.is_file() {
        let yaml = r#"logLevel: info
api: yes
apiAddress: :9997
rtsp: yes
rtspAddress: :8554
rtspTransports: [tcp]
rtmp: yes
rtmpAddress: :1935
hls: yes
hlsAddress: :8888
webrtc: yes
webrtcAddress: :8889
webrtcLocalUDPAddress: :8189
webrtcAdditionalHosts: [127.0.0.1]
srt: no
playback: no
metrics: no
pprof: no
paths:
  demo:
    source: publisher
  cam_in:
    source: publisher
  cam_in_h264:
    source: publisher
  cam_out:
    source: publisher
"#;
        let _ = std::fs::create_dir_all(work_dir);
        let _ = std::fs::write(&out, yaml);
    }
    out
}

/// Prefer supervised ready path when publication is ready.
pub fn browser_safe_path(configured: &str, media: &MediaStatus) -> String {
    if media.publication == PublicationState::Ready {
        if let Some(p) = &media.preferred_webrtc_path {
            return p.clone();
        }
    }
    configured.to_string()
}

/// Default loopback API address.
pub fn default_mediamtx_api_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9997)
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn external_ok_without_h264() {
        let p =
            resolve_media_policy(MediaSourceMode::External, "rtsp://x", "", "cam_in_h264").unwrap();
        assert!(!p.needs_ffmpeg);
        assert_eq!(p.mode, MediaSourceMode::External);
    }

    #[test]
    fn external_rejects_h264_url() {
        let err = resolve_media_policy(
            MediaSourceMode::External,
            "",
            "rtsp://cam/h264",
            "cam_in_h264",
        )
        .unwrap_err();
        assert!(err.contains("external"), "{err}");
    }

    #[test]
    fn copy_requires_h264() {
        assert!(
            resolve_media_policy(MediaSourceMode::Copy, "rtsp://a", "", "cam_in_h264").is_err()
        );
        let p = resolve_media_policy(MediaSourceMode::Copy, "rtsp://a", "rtsp://b", "cam_in_h264")
            .unwrap();
        assert!(p.needs_ffmpeg);
    }

    #[test]
    fn transcode_requires_rtsp() {
        assert!(resolve_media_policy(MediaSourceMode::Transcode, "", "", "cam_in_h264").is_err());
        let p = resolve_media_policy(
            MediaSourceMode::Transcode,
            "rtsp://cam/stream1",
            "",
            "cam_in_h264",
        )
        .unwrap();
        assert!(p.needs_ffmpeg);
    }

    #[test]
    fn path_name_validation() {
        assert!(validate_path_name("cam_in_h264").is_ok());
        assert!(validate_path_name("demo").is_ok());
        assert!(validate_path_name("a/b").is_err());
        assert!(validate_path_name("").is_err());
    }

    #[test]
    fn api_addr_must_be_loopback() {
        assert!(parse_mediamtx_api_addr("127.0.0.1:9997").is_ok());
        assert!(parse_mediamtx_api_addr("[::1]:9997").is_ok());
        assert!(parse_mediamtx_api_addr("0.0.0.0:9997").is_err());
        assert!(parse_mediamtx_api_addr("10.0.0.1:9997").is_err());
        assert!(parse_mediamtx_api_addr("not-an-addr").is_err());
    }

    #[test]
    fn ffmpeg_projection_copy_vs_transcode() {
        let copy = ffmpeg_arg_projection(MediaSourceMode::Copy, "cam_in_h264");
        assert!(copy.contains(&"copy".into()));
        assert!(!copy.contains(&"libx264".into()));
        assert_eq!(copy.iter().filter(|a| *a == "<redacted>").count(), 1);
        assert!(copy.iter().any(|a| a.ends_with("/cam_in_h264")));

        let tc = ffmpeg_arg_projection(MediaSourceMode::Transcode, "cam_in_h264");
        assert!(tc.contains(&"libx264".into()));
        assert!(!tc.contains(&"copy".into()));
        // Projections never embed a real URL.
        assert!(!copy.iter().any(|a| a.contains("rtsp://")));
        assert!(!tc.iter().any(|a| a.contains("rtsp://")));
    }

    #[test]
    fn mode_parse() {
        assert_eq!(
            MediaSourceMode::parse("external").unwrap(),
            MediaSourceMode::External
        );
        assert_eq!(
            MediaSourceMode::parse("COPY").unwrap(),
            MediaSourceMode::Copy
        );
        assert!(MediaSourceMode::parse("auto").is_err());
    }
}

#[cfg(test)]
mod readiness_tests {
    use super::*;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;
    use std::convert::Infallible;
    use tokio::net::TcpListener;

    async fn serve_json(body: &'static str, status: StatusCode) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let io = TokioIo::new(stream);
            let svc = service_fn(move |_req: Request<Incoming>| async move {
                Ok::<_, Infallible>(
                    Response::builder()
                        .status(status)
                        .header("content-type", "application/json")
                        .body(http_body_util::Full::new(bytes::Bytes::from(body)))
                        .unwrap(),
                )
            });
            let _ = http1::Builder::new().serve_connection(io, svc).await;
        });
        tokio::task::yield_now().await;
        addr
    }

    #[tokio::test]
    async fn path_ready_true_when_ready_with_source() {
        let body = r#"{"name":"cam_in_h264","ready":true,"source":{"type":"rtmpSession"}}"#;
        let addr = serve_json(body, StatusCode::OK).await;
        assert!(path_ready(&addr, "cam_in_h264").await.unwrap());
    }

    #[tokio::test]
    async fn path_not_ready_when_flag_false() {
        let body = r#"{"name":"cam_in_h264","ready":false,"source":null}"#;
        let addr = serve_json(body, StatusCode::OK).await;
        assert!(!path_ready(&addr, "cam_in_h264").await.unwrap());
    }

    #[tokio::test]
    async fn path_not_ready_without_source() {
        let body = r#"{"name":"cam_in_h264","ready":true,"source":null}"#;
        let addr = serve_json(body, StatusCode::OK).await;
        assert!(!path_ready(&addr, "cam_in_h264").await.unwrap());
    }

    #[tokio::test]
    async fn path_absent_is_not_ready() {
        let body = r#"{"status":"error"}"#;
        let addr = serve_json(body, StatusCode::NOT_FOUND).await;
        assert!(!path_ready(&addr, "cam_in_h264").await.unwrap());
    }

    #[tokio::test]
    async fn malformed_response_errors() {
        let body = "not-json";
        let addr = serve_json(body, StatusCode::OK).await;
        assert!(path_ready(&addr, "cam_in_h264").await.is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_bundled_mediamtx_if_present() {
        let p = resolve_bin("", "mediamtx");
        if let Some(path) = p {
            assert!(path.is_file(), "{path:?}");
            assert!(path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("mediamtx"));
        }
    }

    #[test]
    fn browser_safe_only_when_ready() {
        let st = MediaStatus {
            preferred_webrtc_path: Some("cam_in_h264".into()),
            publication: PublicationState::Starting,
            ..Default::default()
        };
        assert_eq!(browser_safe_path("demo", &st), "demo");
        let st = MediaStatus {
            preferred_webrtc_path: Some("cam_in_h264".into()),
            publication: PublicationState::Ready,
            ..Default::default()
        };
        assert_eq!(browser_safe_path("demo", &st), "cam_in_h264");
    }
}
