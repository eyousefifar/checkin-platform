//! Axum REST + WebSocket API for PKSP edge.

mod auth;
mod error;
mod routes;
mod state;

pub use state::AppState;

use axum::routing::{get, post};
use axum::Router;
use pksp_db::{connect_pool, list_cameras, Settings};
use pksp_media::{MediaConfig, MediaSupervisor};
use pksp_vision::{
    reload_gallery, start_vision_worker, FaceEngine, Gallery, MockFaceEngine, OrtFaceEngine,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

pub async fn serve(settings: Settings) -> anyhow::Result<()> {
    let settings = Arc::new(settings);
    let pool = connect_pool(&settings).await?;

    let engine: Arc<dyn FaceEngine> = if settings.mock_vision {
        Arc::new(MockFaceEngine::new(settings.embedding_dim))
    } else {
        let model_dir = settings.model_dir();
        let ort =
            OrtFaceEngine::try_load_with(&model_dir, settings.det_size, &settings.onnx_providers);
        if ort.ready() {
            info!(
                provider = ort.execution_provider(),
                "ONNX buffalo_l engine ready"
            );
            Arc::new(ort)
        } else if settings.require_real_vision {
            anyhow::bail!(
                "REQUIRE_REAL_VISION=true but ONNX not ready under {}",
                model_dir.display()
            );
        } else {
            warn!("ONNX models not ready; falling back to mock engine");
            Arc::new(MockFaceEngine::new(settings.embedding_dim))
        }
    };

    let gallery = Arc::new(RwLock::new(Gallery::empty(
        settings.match_threshold,
        settings.match_margin,
    )));
    reload_gallery(&pool, &gallery, &settings).await?;

    let (tx, _) = broadcast::channel::<serde_json::Value>(256);

    // Media supervisor: bundled MediaMTX + optional ffmpeg under apps/edge/bin/
    // Prefer explicit H.264 RTSP; else transcoder for H.265 / stream1 / FORCE_TRANSCODE.
    let work_dir = settings.data_dir.clone();
    let force_tc = std::env::var("FORCE_TRANSCODE").as_deref() == Ok("true");
    let need_transcode =
        pksp_media::should_transcode(&settings.cam_in_rtsp, &settings.cam_in_h264_rtsp, force_tc);
    let media_cfg = MediaConfig {
        mediamtx_bin: settings.mediamtx_bin.clone(),
        config_path: settings.mediamtx_config.clone(),
        ffmpeg_bin: settings.ffmpeg_bin.clone(),
        // Prefer H.264 source for transcoder input when available, else high-res H.265
        h265_rtsp: if need_transcode {
            Some(settings.cam_in_rtsp.clone())
        } else {
            None
        },
        h264_publish_path: "cam_in_h264".into(),
        work_dir,
    };
    let media = Arc::new(MediaSupervisor::new(media_cfg));
    media.start();
    let media_status = media.status_handle();

    let cams = list_cameras(&pool, true).await?;
    let cam_ids: Vec<String> = cams.iter().map(|c| c.id.clone()).collect();
    let mut camera_rtsps: HashMap<String, String> = HashMap::new();
    for c in &cams {
        if !c.rtsp_url.is_empty() {
            camera_rtsps.insert(c.id.clone(), c.rtsp_url.clone());
        }
    }
    if camera_rtsps.is_empty() && !settings.cam_in_rtsp.is_empty() {
        camera_rtsps.insert("cam_in".into(), settings.cam_in_rtsp.clone());
    }

    let vision = if settings.vision_enabled {
        Some(start_vision_worker(
            pool.clone(),
            settings.clone(),
            engine.clone(),
            gallery.clone(),
            tx.clone(),
            if cam_ids.is_empty() {
                vec!["cam_in".into()]
            } else {
                cam_ids
            },
            camera_rtsps,
        ))
    } else {
        None
    };

    let state = AppState {
        settings: settings.clone(),
        pool,
        gallery,
        engine,
        hub_tx: tx,
        vision,
        media_status,
    };

    let app = app(state.clone());

    let addr: SocketAddr = settings
        .bind_addr
        .parse()
        .unwrap_or_else(|_| "0.0.0.0:8000".parse().unwrap());
    info!("pksp listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Graceful shutdown: stop vision + media children, then drain HTTP
    let media_for_stop = media.clone();
    let vision_for_stop = state.vision.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            info!("shutdown signal received");
            if let Some(v) = vision_for_stop {
                v.stop();
            }
            media_for_stop.stop();
            // brief pause for SQLite writers / child reaps
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        })
        .await?;
    info!("pksp stopped cleanly");
    Ok(())
}

/// Build the HTTP router for the given app state.
///
/// Safe for in-process tests: does not bind a listener or start media/vision workers.
pub fn app(state: AppState) -> Router {
    use axum::http::{header, HeaderValue, Method};
    let cors = if state.settings.cors_origins.iter().any(|o| o == "*") {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
    } else {
        let origins: Vec<HeaderValue> = state
            .settings
            .cors_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
            .allow_credentials(true)
    };

    Router::new()
        .route("/api/health", get(routes::health))
        .route("/api/auth/login", post(routes::login))
        .route(
            "/api/employees",
            get(routes::list_employees).post(routes::create_employee),
        )
        .route(
            "/api/employees/{id}",
            get(routes::get_employee).patch(routes::update_employee),
        )
        .route("/api/employees/{id}/images", post(routes::upload_images))
        .route(
            "/api/employees/{id}/recompute-embedding",
            post(routes::recompute_embedding),
        )
        .route("/api/attendance/daily", get(routes::daily))
        .route("/api/attendance/daily.csv", get(routes::daily_csv))
        .route("/api/attendance/events", get(routes::events))
        .route("/api/cameras", get(routes::list_cameras_route))
        .route("/api/ws/live", get(routes::ws_live))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
