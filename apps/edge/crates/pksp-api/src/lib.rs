//! Axum REST + WebSocket API for PKSP edge.

mod auth;
mod error;
mod routes;
mod state;

pub use state::AppState;

use axum::routing::{get, post};
use axum::Router;
use pksp_db::{connect_pool, list_cameras, Settings};
use pksp_media::MediaSupervisor;
use pksp_vision::{reload_gallery, start_vision_worker, FaceEngine, Gallery, OrtFaceEngine};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

pub async fn serve(settings: Settings) -> anyhow::Result<()> {
    // Fail closed on APP_TIMEZONE before DB, models, media, or listener.
    // Invalid IANA names must not silently fall back to UTC.
    let _ = pksp_db::local_date_str(chrono::Utc::now(), &settings.app_timezone)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    // Fail closed before DB, models, media children, or vision workers start.
    let addr = settings
        .validate_startup()
        .map_err(|e| anyhow::anyhow!(e))?;
    let settings = Arc::new(settings);
    let model_dir = settings.model_dir();
    let ort = OrtFaceEngine::try_load_with(&model_dir, settings.det_size, &settings.onnx_providers);
    if !ort.ready() {
        anyhow::bail!(
            "buffalo_l ONNX models or sessions unavailable under {}",
            model_dir.display()
        );
    }
    info!(
        provider = ort.execution_provider(),
        "ONNX buffalo_l engine ready"
    );
    let engine: Arc<dyn FaceEngine> = Arc::new(ort);

    let pool = connect_pool(&settings).await?;

    let gallery = Arc::new(RwLock::new(Gallery::empty(
        settings.match_threshold,
        settings.match_margin,
    )));
    reload_gallery(&pool, &gallery, &settings).await?;

    let cams = list_cameras(&pool, true).await?;
    let cam_ids: Vec<String> = if cams.is_empty() {
        vec!["cam_in".into()]
    } else {
        cams.iter().map(|c| c.id.clone()).collect()
    };
    let mut camera_rtsps: HashMap<String, String> = cams
        .iter()
        .map(|c| (c.id.clone(), c.rtsp_url.clone()))
        .collect();
    if cams.is_empty() {
        camera_rtsps.insert("cam_in".into(), settings.cam_in_rtsp.clone());
    }
    if let Some(camera_id) = cam_ids.iter().find(|camera_id| {
        camera_rtsps
            .get(*camera_id)
            .is_none_or(|url| url.trim().is_empty())
    }) {
        anyhow::bail!("enabled camera {camera_id} has no RTSP URL");
    }

    let (tx, _) = broadcast::channel::<serde_json::Value>(256);

    // Media supervisor: bundled MediaMTX + optional ffmpeg under apps/edge/bin/
    // Publication policy is explicit (MEDIA_SOURCE_MODE); never inferred from URLs alone.
    let work_dir = settings.data_dir.clone();
    let mode = pksp_media::MediaSourceMode::parse(&settings.media_source_mode)
        .map_err(|e| anyhow::anyhow!(e))?;
    let api_addr = pksp_media::parse_mediamtx_api_addr(&settings.mediamtx_api_addr)
        .map_err(|e| anyhow::anyhow!(e))?;
    let media_cfg = pksp_media::build_media_config(
        mode,
        &settings.cam_in_rtsp,
        &settings.cam_in_h264_rtsp,
        &settings.media_publish_path,
        settings.mediamtx_bin.clone(),
        settings.mediamtx_config.clone(),
        settings.ffmpeg_bin.clone(),
        work_dir,
        api_addr,
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let media = Arc::new(MediaSupervisor::new(media_cfg));
    media.start();
    let media_status = media.status_handle();

    let vision = if settings.vision_enabled {
        Some(start_vision_worker(
            pool.clone(),
            settings.clone(),
            engine.clone(),
            gallery.clone(),
            tx.clone(),
            cam_ids,
            camera_rtsps,
        )?)
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
    use axum::extract::DefaultBodyLimit;
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

    let enroll_body_limit = state.settings.max_enroll_upload_bytes;

    Router::new()
        .route("/api/health", get(routes::health))
        .route("/api/auth/login", post(routes::login))
        .route(
            "/api/employees",
            get(routes::list_employees).post(routes::create_employee),
        )
        .route(
            "/api/employees/{id}",
            get(routes::get_employee)
                .patch(routes::update_employee)
                .delete(routes::deactivate_employee),
        )
        .route(
            "/api/employees/{id}/images",
            post(routes::upload_images).layer(DefaultBodyLimit::max(enroll_body_limit)),
        )
        .route(
            "/api/employees/{id}/recompute-embedding",
            post(routes::recompute_embedding),
        )
        .route(
            "/api/enrollment/analyze",
            post(routes::analyze_enrollment_frame).layer(DefaultBodyLimit::max(enroll_body_limit)),
        )
        .route("/api/attendance/daily", get(routes::daily))
        .route("/api/attendance/daily.csv", get(routes::daily_csv))
        .route("/api/attendance/events", get(routes::events))
        .route(
            "/api/attendance/events/{id}/snapshot",
            get(routes::event_snapshot),
        )
        .route("/api/cameras", get(routes::list_cameras_route))
        .route("/api/ws/live", get(routes::ws_live))
        .layer(cors)
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &axum::http::Request<_>| {
                tracing::info_span!(
                    "http_request",
                    method = %req.method(),
                    path = %http_request_path(req.uri()),
                )
            }),
        )
        .with_state(state)
}

/// Span/path label from a request URI — path only, never query text.
pub fn http_request_path(uri: &axum::http::Uri) -> &str {
    uri.path()
}
