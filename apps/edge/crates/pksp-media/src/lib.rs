//! Media plane: supervise **bundled** MediaMTX (+ optional ffmpeg transcoder).
//!
//! Binaries live under `apps/edge/bin/` (see `scripts/download-binaries.sh`).
//! No separate manual MediaMTX/GStreamer processes or `/tmp` scripts.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct MediaConfig {
    /// Path or name; empty / "mediamtx" → auto-resolve bundled bin.
    pub mediamtx_bin: String,
    pub config_path: PathBuf,
    /// ffmpeg for optional H.265→H.264 (empty → auto-resolve).
    pub ffmpeg_bin: String,
    /// Optional RTSP source that needs transcoding (e.g. H.265 camera).
    pub h265_rtsp: Option<String>,
    pub h264_publish_path: String,
    /// Working directory for child processes (logs, relative config).
    pub work_dir: PathBuf,
}

#[derive(Debug, Default, Clone)]
pub struct MediaStatus {
    pub mediamtx_running: bool,
    pub transcoder_running: bool,
    pub last_error: Option<String>,
    pub preferred_webrtc_path: Option<String>,
    pub mediamtx_path: Option<String>,
    pub ffmpeg_path: Option<String>,
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
        Self {
            cfg,
            stop: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(MediaStatus::default())),
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
        if self.cfg.h265_rtsp.is_some() {
            let this = self.clone();
            tokio::spawn(async move {
                this.run_transcoder_loop().await;
            });
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    async fn run_mediamtx_loop(&self) {
        while !self.stop.load(Ordering::SeqCst) {
            let Some(bin) = resolve_bin(&self.cfg.mediamtx_bin, "mediamtx") else {
                let msg = format!(
                    "mediamtx not found (looked for bundled apps/edge/bin/mediamtx, {:?}, PATH). Run apps/edge/scripts/download-binaries.sh",
                    self.cfg.mediamtx_bin
                );
                warn!("{msg}");
                let mut st = self.status.lock().await;
                st.mediamtx_running = false;
                st.last_error = Some(msg);
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                continue;
            };

            // Ensure config exists
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
                        st.last_error = None;
                    }
                    *self.child.lock().await = Some(child);
                    if let Some(mut c) = self.child.lock().await.take() {
                        let _ = c.wait().await;
                    }
                    self.status.lock().await.mediamtx_running = false;
                    if self.stop.load(Ordering::SeqCst) {
                        break;
                    }
                    warn!("MediaMTX exited; restarting in 2s");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                Err(e) => {
                    let msg = format!("failed to spawn mediamtx {}: {e}", bin.display());
                    warn!("{msg}");
                    self.status.lock().await.last_error = Some(msg);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn run_transcoder_loop(&self) {
        let Some(rtsp) = self.cfg.h265_rtsp.clone() else {
            return;
        };
        let path = self.cfg.h264_publish_path.clone();

        // Wait briefly for MediaMTX RTMP to come up
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        while !self.stop.load(Ordering::SeqCst) {
            let Some(ff) = resolve_bin(&self.cfg.ffmpeg_bin, "ffmpeg") else {
                let msg = "ffmpeg not found in apps/edge/bin or PATH; H.265→H.264 disabled (use H.264 camera or run download-binaries.sh)".to_string();
                warn!("{msg}");
                let mut st = self.status.lock().await;
                st.transcoder_running = false;
                st.last_error = Some(msg);
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            };
            {
                let mut st = self.status.lock().await;
                st.ffmpeg_path = Some(ff.display().to_string());
            }

            let rtmp = format!("rtmp://127.0.0.1:1935/{path}");
            info!("starting supervised ffmpeg transcoder → {rtmp}");

            // Low-latency H.264 publish into MediaMTX publisher path
            let mut cmd = Command::new(&ff);
            cmd.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-rtsp_transport",
                "tcp",
                "-i",
                &rtsp,
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-tune",
                "zerolatency",
                "-b:v",
                "1800k",
                "-f",
                "flv",
                &rtmp,
            ]);
            cmd.stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);

            match cmd.spawn() {
                Ok(child) => {
                    {
                        let mut st = self.status.lock().await;
                        st.transcoder_running = true;
                        st.preferred_webrtc_path = Some(path.clone());
                    }
                    *self.ffmpeg_child.lock().await = Some(child);
                    if let Some(mut c) = self.ffmpeg_child.lock().await.take() {
                        let _ = c.wait().await;
                    }
                    self.status.lock().await.transcoder_running = false;
                    if self.stop.load(Ordering::SeqCst) {
                        break;
                    }
                    warn!("ffmpeg transcoder exited; restarting in 5s");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
                Err(e) => {
                    warn!("ffmpeg spawn failed: {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
        }
    }
}

/// Resolve a tool name against env path, bundled `apps/edge/bin`, exe dir, then PATH.
pub fn resolve_bin(configured: &str, default_name: &str) -> Option<PathBuf> {
    let candidates: Vec<String> = if configured.is_empty() || configured == default_name {
        vec![default_name.to_string()]
    } else {
        vec![configured.to_string(), default_name.to_string()]
    };

    for name in candidates {
        // Absolute or relative explicit path
        let p = PathBuf::from(&name);
        if p.is_file() {
            return Some(p);
        }

        // Bundled under apps/edge/bin (from this crate: crates/pksp-media → ../../bin)
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

        // Cwd-relative apps/edge/bin
        for rel in [
            PathBuf::from("bin").join(&name),
            PathBuf::from("apps/edge/bin").join(&name),
        ] {
            if rel.is_file() {
                return Some(rel);
            }
        }

        // Next to current executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let cand = dir.join(&name);
                if cand.is_file() {
                    return Some(cand);
                }
                // cargo run: target/debug/pksp → ../../../bin
                let up = dir.join("../../../bin").join(&name);
                if up.is_file() {
                    return Some(up);
                }
            }
        }

        // PATH
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
    // Try bundled edge config next to crate
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
    // Write minimal config into work_dir
    let out = work_dir.join("mediamtx.yml");
    if !out.is_file() {
        let yaml = r#"logLevel: info
api: yes
apiAddress: :9997
rtsp: yes
rtspAddress: :8554
rtmp: yes
rtmpAddress: :1935
hls: yes
hlsAddress: :8888
webrtc: yes
webrtcAddress: :8889
paths:
  demo:
    source: publisher
  cam_in:
    source: publisher
  cam_in_h264:
    source: publisher
"#;
        let _ = std::fs::create_dir_all(work_dir);
        let _ = std::fs::write(&out, yaml);
    }
    out
}

/// Prefer supervised H.264 publish path when transcoder is (or will be) active.
pub fn browser_safe_path(configured: &str, media: &MediaStatus) -> String {
    if let Some(p) = &media.preferred_webrtc_path {
        return p.clone();
    }
    configured.to_string()
}

/// Whether settings imply we should run H.265→H.264 transcoder.
pub fn should_transcode(cam_in_rtsp: &str, cam_in_h264_rtsp: &str, force: bool) -> bool {
    if !cam_in_h264_rtsp.is_empty() {
        return false; // native H.264 preferred
    }
    force || cam_in_rtsp.contains("stream1") || cam_in_rtsp.contains("h265")
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn no_transcode_when_h264_url_set() {
        assert!(!should_transcode(
            "rtsp://cam/stream1",
            "rtsp://cam/stream2",
            true
        ));
    }

    #[test]
    fn transcode_stream1() {
        assert!(should_transcode("rtsp://cam/stream1", "", false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_bundled_mediamtx_if_present() {
        // May pass only after download-binaries.sh
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
}
