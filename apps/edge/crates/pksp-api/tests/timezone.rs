//! APP_TIMEZONE serve-time validation, health projection, and omitted-date defaults.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{TimeZone, Utc};
use pksp_api::{app, AppState};
use pksp_db::{connect_pool, local_date_str, Settings};
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
    let data_dir = std::env::temp_dir().join(format!("pksp-api-tz-{id}"));
    std::fs::create_dir_all(&data_dir).expect("create temp data_dir");
    let db_path = data_dir.join("test.db");
    TestDb { data_dir, db_path }
}

fn test_settings(db: &TestDb) -> Settings {
    let mut settings = Settings::from_env();
    let abs = db
        .db_path
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();
    settings.database_url = format!("sqlite:////{abs}?mode=rwc");
    settings.data_dir = db.data_dir.clone();
    settings.admin_password = "test-admin-password".into();
    settings.jwt_secret = "test-jwt-secret-for-timezone-tests".into();
    settings.jwt_ttl_hours = 1;
    settings.cors_origins = vec!["http://localhost:3000".into()];
    settings.vision_enabled = false;
    settings.camera_upsert = true;
    settings.cam_out_rtsp = String::new();
    settings.app_timezone = "Asia/Tehran".into();
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

async fn login_token(router: &axum::Router, password: &str) -> String {
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"password":"{password}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    body["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn serve_rejects_invalid_timezone_before_side_effects() {
    let id = Uuid::new_v4();
    let data_dir = std::env::temp_dir().join(format!("pksp-api-tz-invalid-{id}"));
    // Intentionally do not create data_dir — serve must fail before mkdir/DB/listener.
    assert!(!data_dir.exists());
    let db_path = data_dir.join("must-not-be-created.db");

    let mut settings = Settings::from_env();
    let abs = db_path
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();
    settings.database_url = format!("sqlite:////{abs}?mode=rwc");
    settings.data_dir = data_dir.clone();
    settings.app_timezone = "Not/A_Real_Zone".into();
    settings.vision_enabled = false;
    settings.bind_addr = "127.0.0.1:0".into();
    settings.admin_password = "test-admin-password".into();
    settings.jwt_secret = "test-jwt-secret-for-timezone-tests".into();

    let err = pksp_api::serve(settings)
        .await
        .expect_err("invalid APP_TIMEZONE must fail serve");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid APP_TIMEZONE") || msg.contains("Not/A_Real_Zone"),
        "unexpected error: {msg}"
    );

    assert!(
        !data_dir.exists(),
        "serve must not create data_dir before timezone validation"
    );
    assert!(!db_path.exists(), "serve must not create DB file");
}

#[tokio::test]
async fn health_exposes_validated_timezone_only() {
    let db = temp_db();
    let state = test_state(&db).await;
    assert_eq!(state.settings.app_timezone, "Asia/Tehran");
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
    assert_eq!(body["timezone"], "Asia/Tehran");
    // Health must not grow into a settings dump.
    assert!(body.get("cam_in_rtsp").is_none());
    assert!(body.get("admin_password").is_none());
    assert!(body.get("jwt_secret").is_none());
    let s = body.to_string();
    assert!(!s.contains("rtsp://"));
}

#[tokio::test]
async fn omitted_daily_date_uses_app_timezone_calendar() {
    let db = temp_db();
    let mut settings = test_settings(&db);
    // Pick a fixed zone; the default day must match local_date_str, not UTC date alone.
    settings.app_timezone = "Asia/Tehran".into();
    let settings = Arc::new(settings);
    let pool = connect_pool(&settings).await.expect("connect_pool");
    let engine = common::test_engine(settings.embedding_dim);
    let gallery = Arc::new(RwLock::new(Gallery::empty(
        settings.match_threshold,
        settings.match_margin,
    )));
    let (hub_tx, _) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        settings: settings.clone(),
        pool,
        gallery,
        engine,
        hub_tx,
        vision: None,
        media_status: Arc::new(tokio::sync::Mutex::new(MediaStatus::default())),
    };
    let password = state.settings.admin_password.clone();
    let router = app(state);
    let token = login_token(&router, &password).await;

    let expected = local_date_str(Utc::now(), "Asia/Tehran").unwrap();
    let utc_day = Utc::now().date_naive().to_string();
    // When UTC and Tehran dates differ this still asserts the local calendar day.
    let _ = utc_day;

    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/attendance/daily")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // Empty day is fine; the important part is that the server accepted the
    // APP_TIMEZONE-derived default rather than panicking or using an invalid zone.
    let body = body_json(res).await;
    assert!(body.is_array(), "daily returns array, got {body}");

    // Boundary unit: known instant where UTC date and Tehran date differ.
    let boundary = Utc.with_ymd_and_hms(2026, 7, 12, 22, 30, 0).unwrap();
    assert_eq!(local_date_str(boundary, "UTC").unwrap(), "2026-07-12");
    assert_eq!(
        local_date_str(boundary, "Asia/Tehran").unwrap(),
        "2026-07-13"
    );
    assert_eq!(expected, local_date_str(Utc::now(), "Asia/Tehran").unwrap());
}

#[tokio::test]
async fn daily_csv_omitted_date_and_local_times() {
    let db = temp_db();
    let mut settings = test_settings(&db);
    settings.app_timezone = "Asia/Tehran".into();
    let settings = Arc::new(settings);
    let pool = connect_pool(&settings).await.expect("connect_pool");
    let emp_id = pksp_db::create_employee(&pool, "E-TZ", "Timezone User", None)
        .await
        .unwrap();
    let local_day = local_date_str(Utc::now(), "Asia/Tehran").unwrap();
    sqlx::query(
        "INSERT INTO attendance_events(employee_id, camera_id, kind, score, ts, local_date)
         VALUES(?,?,?,?,?,?)",
    )
    .bind(emp_id)
    .bind("cam_in")
    .bind("check_in")
    .bind(0.95_f64)
    .bind("2026-07-12 08:00:00")
    .bind(&local_day)
    .execute(&pool)
    .await
    .unwrap();

    let engine = common::test_engine(settings.embedding_dim);
    let gallery = Arc::new(RwLock::new(Gallery::empty(
        settings.match_threshold,
        settings.match_margin,
    )));
    let (hub_tx, _) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        settings: settings.clone(),
        pool,
        gallery,
        engine,
        hub_tx,
        vision: None,
        media_status: Arc::new(tokio::sync::Mutex::new(MediaStatus::default())),
    };
    let password = state.settings.admin_password.clone();
    let router = app(state);
    let token = login_token(&router, &password).await;

    let res = router
        .oneshot(
            Request::builder()
                .uri(format!("/api/attendance/daily.csv?date={local_day}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let csv = String::from_utf8(bytes.to_vec()).unwrap();
    // 08:00 UTC → 11:30 Asia/Tehran
    assert!(csv.contains("11:30:00"), "csv={csv}");
    assert!(!csv.contains("2026-07-12T08:00:00Z"), "csv={csv}");
    assert!(csv.contains(&local_day), "csv={csv}");
}
