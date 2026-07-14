//! In-process API contract tests — no listener, media workers, or vision processes.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use pksp_api::{app, AppState};
use pksp_db::{connect_pool, Settings};
use pksp_media::MediaStatus;
use pksp_vision::Gallery;
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
    settings.vision_enabled = false;
    settings.camera_upsert = true;
    settings.cam_out_rtsp = String::new();
    settings
}

async fn test_state(db: &TestDb) -> AppState {
    let settings = Arc::new(test_settings(db));
    let pool = connect_pool(&settings).await.expect("connect_pool");
    let engine = common::test_engine(settings.embedding_dim);
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
    // Validated timezone and fixed operational model identity are public.
    assert!(body["timezone"].is_string());
    assert!(!body["timezone"].as_str().unwrap().is_empty());
    assert_eq!(body["vision_model"], "buffalo_l");
    // Distinguish process/configuration from a ready publisher.
    assert_eq!(body["media"]["publication"], "unavailable");
    assert!(body["media"]["preferred_webrtc_path"].is_null());
    // Never leak source URLs in health.
    let media_s = body["media"].to_string();
    assert!(!media_s.contains("rtsp://"));
}

#[tokio::test]
async fn health_media_publication_ready_exposes_path_without_source_url() {
    use pksp_media::PublicationState;
    let db = temp_db();
    let state = test_state(&db).await;
    {
        let mut media = state.media_status.lock().await;
        media.mediamtx_running = true;
        media.transcoder_running = true;
        media.publication = PublicationState::Ready;
        media.preferred_webrtc_path = Some("cam_in_h264".into());
        media.source_mode = Some("transcode".into());
    }
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
    assert_eq!(body["media"]["publication"], "ready");
    assert_eq!(body["media"]["preferred_webrtc_path"], "cam_in_h264");
    assert_eq!(body["media"]["source_mode"], "transcode");
    let media_s = body["media"].to_string();
    assert!(!media_s.contains("rtsp://"));
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

// ── Event snapshots ──────────────────────────────────────────────────────────

use axum::http::header;
use image::{ImageBuffer, ImageFormat, Rgb};
use pksp_db::{
    attach_event_snapshot, commit_identity, create_employee, event_snapshot_rel_path,
    get_event_snapshot, resolve_event_snapshot_path, CommitOutcome,
};
use std::io::Cursor;

fn solid_jpeg(w: u32, h: u32, v: u8) -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |_, _| Rgb([v, v, v]));
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg)
        .unwrap();
    buf
}

async fn login_token(router: &axum::Router, password: &str) -> String {
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"password":"{password}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn event_snapshot_404_when_missing() {
    let db = temp_db();
    let state = test_state(&db).await;
    let router = app(state);
    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/attendance/events/99999/snapshot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn event_snapshot_serves_jpeg_no_store() {
    let db = temp_db();
    let state = test_state(&db).await;
    let pool = state.pool.clone();
    let settings = state.settings.clone();
    let password = state.settings.admin_password.clone();

    let eid = create_employee(&pool, "SNAP1", "Snap User", None)
        .await
        .unwrap();
    let outcome = commit_identity(&pool, eid, "cam_in", 0.95, Some(1), 0.0, 0.0, "UTC")
        .await
        .unwrap();
    let CommitOutcome::Committed { event_id, .. } = outcome else {
        panic!("expected commit");
    };

    let rel = event_snapshot_rel_path(event_id);
    let events = settings.data_dir.join("events");
    std::fs::create_dir_all(&events).unwrap();
    let jpeg = solid_jpeg(32, 32, 180);
    std::fs::write(settings.data_dir.join(&rel), &jpeg).unwrap();
    attach_event_snapshot(&pool, event_id, &rel, [0.1, 0.2, 0.5, 0.6])
        .await
        .unwrap();

    let router = app(state);
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/attendance/events/{event_id}/snapshot"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/jpeg"
    );
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    assert_eq!(
        res.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(bytes.as_ref(), jpeg.as_slice());

    // History includes additive snapshot fields
    let token = login_token(&router, &password).await;
    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/attendance/events")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    let arr = v.as_array().unwrap();
    let row = arr.iter().find(|r| r["id"] == event_id).expect("row");
    assert_eq!(
        row["snapshot_url"],
        format!("/api/attendance/events/{event_id}/snapshot")
    );
    assert_eq!(row["bbox"][0], 0.1);
    assert_eq!(row["bbox"][3], 0.6);
}

#[tokio::test]
async fn event_history_null_snapshot_when_absent() {
    let db = temp_db();
    let state = test_state(&db).await;
    let pool = state.pool.clone();
    let password = state.settings.admin_password.clone();
    let eid = create_employee(&pool, "SNAP2", "No Snap", None)
        .await
        .unwrap();
    let outcome = commit_identity(&pool, eid, "cam_in", 0.9, None, 0.0, 0.0, "UTC")
        .await
        .unwrap();
    let CommitOutcome::Committed { event_id, .. } = outcome else {
        panic!("expected commit");
    };
    let meta = get_event_snapshot(&pool, event_id).await.unwrap().unwrap();
    assert!(meta.snapshot_path.is_none());

    let router = app(state);
    let token = login_token(&router, &password).await;
    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/attendance/events")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let v = body_json(res).await;
    let row = v
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == event_id)
        .unwrap();
    assert!(row["snapshot_url"].is_null());
    assert!(row["bbox"].is_null());
}

#[test]
fn resolve_snapshot_path_rejects_traversal() {
    let db = temp_db();
    let settings = test_settings(&db);
    assert!(resolve_event_snapshot_path(&settings, "../etc/passwd").is_err());
    assert!(resolve_event_snapshot_path(&settings, "enroll/1/x.jpg").is_err());
    assert!(resolve_event_snapshot_path(&settings, "/absolute/x.jpg").is_err());
    let ok = resolve_event_snapshot_path(&settings, "events/42.jpg").unwrap();
    assert!(ok.ends_with("events/42.jpg"));
}

#[cfg(unix)]
#[test]
fn resolve_snapshot_path_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let db = temp_db();
    let settings = test_settings(&db);
    let events = settings.data_dir.join("events");
    std::fs::create_dir_all(&events).unwrap();
    let outside = settings.data_dir.join("outside.jpg");
    std::fs::write(&outside, b"not an event snapshot").unwrap();
    symlink(&outside, events.join("42.jpg")).unwrap();

    assert!(resolve_event_snapshot_path(&settings, "events/42.jpg").is_err());
}
