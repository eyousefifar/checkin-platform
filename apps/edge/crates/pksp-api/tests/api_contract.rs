//! In-process API contract tests — no listener, media workers, or vision processes.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use pksp_api::{app, AppState};
use pksp_db::{connect_pool, Settings};
use pksp_media::MediaStatus;
use pksp_vision::{Gallery, MockFaceEngine};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tower::ServiceExt;
use uuid::Uuid;

struct TestDb {
    data_dir: PathBuf,
    db_path: PathBuf,
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

fn temp_db() -> TestDb {
    let id = Uuid::new_v4();
    let data_dir = std::env::temp_dir().join(format!("pksp-api-test-{id}"));
    std::fs::create_dir_all(&data_dir).expect("create temp data_dir");
    let db_path = data_dir.join("test.db");
    TestDb { data_dir, db_path }
}

fn test_settings(db: &TestDb) -> Settings {
    let mut settings = Settings::from_env();
    // Four-slash sqlite URL → absolute path (three-slash is treated as relative).
    let abs = db
        .db_path
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();
    settings.database_url = format!("sqlite:////{abs}?mode=rwc");
    settings.data_dir = db.data_dir.clone();
    settings.admin_password = "test-admin-password".into();
    settings.jwt_secret = "test-jwt-secret-for-api-contract".into();
    settings.jwt_ttl_hours = 1;
    settings.cors_origins = vec!["http://localhost:3000".into()];
    settings.mock_vision = true;
    settings.vision_enabled = false;
    settings.require_real_vision = false;
    settings.camera_upsert = true;
    settings.cam_out_rtsp = String::new();
    settings
}

async fn test_state(db: &TestDb) -> AppState {
    let settings = Arc::new(test_settings(db));
    let pool = connect_pool(&settings).await.expect("connect_pool");
    let engine: Arc<dyn pksp_vision::FaceEngine> =
        Arc::new(MockFaceEngine::new(settings.embedding_dim));
    let gallery = Arc::new(RwLock::new(Gallery::empty(
        settings.match_threshold,
        settings.match_margin,
    )));
    let (hub_tx, _) = tokio::sync::broadcast::channel(16);
    AppState {
        settings,
        pool,
        gallery,
        engine,
        hub_tx,
        vision: None,
        media_status: Arc::new(tokio::sync::Mutex::new(MediaStatus::default())),
    }
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn health_returns_200_with_status_cameras_media() {
    let db = temp_db();
    let state = test_state(&db).await;
    let router = app(state);

    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["status"], "ok");
    assert!(body.get("cameras").is_some(), "missing cameras");
    assert!(body.get("media").is_some(), "missing media");
    assert!(body["cameras"].is_array());
    assert!(body["media"].is_object());
}

#[tokio::test]
async fn protected_route_without_bearer_returns_401_with_detail() {
    let db = temp_db();
    let state = test_state(&db).await;
    let router = app(state);

    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/employees")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert!(
        body.get("detail").is_some(),
        "401 body must include detail, got {body}"
    );
}

#[tokio::test]
async fn invalid_login_returns_401() {
    let db = temp_db();
    let state = test_state(&db).await;
    let router = app(state);

    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"password":"definitely-wrong"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert!(body.get("detail").is_some());
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let db = temp_db();
    let state = test_state(&db).await;
    let router = app(state);

    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn router_construction_starts_no_listener_worker_or_child() {
    let db = temp_db();
    let state = test_state(&db).await;
    // Precondition: no vision handle, media idle
    assert!(state.vision.is_none());
    {
        let media = state.media_status.lock().await;
        assert!(!media.mediamtx_running);
        assert!(!media.transcoder_running);
        assert!(media.last_error.is_none());
    }

    // Constructing the router must not bind a socket or spawn workers
    let router = app(state.clone());
    let _ = router
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(state.vision.is_none());
    let media = state.media_status.lock().await;
    assert!(
        !media.mediamtx_running && !media.transcoder_running,
        "health must not start media processes"
    );
}
