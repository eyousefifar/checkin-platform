//! Enrollment boundary tests — limits, 404, cumulative upload, recompute integrity.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use image::{ImageBuffer, ImageFormat, Rgb};
use pksp_api::{app, AppState};
use pksp_db::{connect_pool, create_employee, list_employee_images, Settings};
use pksp_media::MediaStatus;
use pksp_vision::Gallery;
use serde_json::Value;
use std::io::Cursor;
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
    let data_dir = std::env::temp_dir().join(format!("pksp-enroll-api-{id}"));
    std::fs::create_dir_all(&data_dir).unwrap();
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
    settings.jwt_secret = "test-jwt-secret-for-enrollment".into();
    settings.jwt_ttl_hours = 1;
    settings.cors_origins = vec!["http://localhost:3000".into()];
    settings.vision_enabled = false;
    settings.camera_upsert = true;
    settings.cam_out_rtsp = String::new();
    settings.embedding_dim = 16;
    settings.min_enroll_images = 1;
    settings.min_face_px = 10;
    settings.min_det_score = 0.1;
    settings.pose_max_yaw = 0.0;
    settings.blur_min_var = 0.0;
    settings.max_enroll_files = 3;
    settings.max_enroll_file_bytes = 50_000;
    settings.max_enroll_upload_bytes = 200_000;
    settings.max_enroll_image_dim = 512;
    settings.max_enroll_pixels = 200_000;
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

fn solid_png(w: u32, h: u32, v: u8) -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |_, _| Rgb([v, v, v]));
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
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

fn multipart_body(parts: &[(&str, &[u8], &str)]) -> (String, Vec<u8>) {
    let boundary = format!("----pksp{}", Uuid::new_v4().simple());
    let mut body = Vec::new();
    for (name, data, filename) in parts {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (boundary, body)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn enrollment_limits_empty_batch_400() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let eid = create_employee(&state.pool, "L1", "Limit", None)
        .await
        .unwrap();
    let router = app(state);
    let token = login_token(&router, &password).await;

    let boundary = "----empty";
    let body = format!("--{boundary}--\r\n").into_bytes();
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/employees/{eid}/images"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let v = body_json(res).await;
    assert!(v["detail"].as_str().unwrap().contains("no images"), "{v}");
}

#[tokio::test]
async fn enrollment_limits_too_many_files_400() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let max_files = state.settings.max_enroll_files;
    let eid = create_employee(&state.pool, "L2", "Many", None)
        .await
        .unwrap();
    let router = app(state);
    let token = login_token(&router, &password).await;

    let png = solid_png(40, 40, 120);
    let mut parts = Vec::new();
    let owned: Vec<(String, Vec<u8>, String)> = (0..=max_files)
        .map(|i| (format!("f{i}"), png.clone(), format!("f{i}.png")))
        .collect();
    for (n, d, f) in &owned {
        parts.push((n.as_str(), d.as_slice(), f.as_str()));
    }
    let (boundary, body) = multipart_body(&parts);
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/employees/{eid}/images"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let v = body_json(res).await;
    assert!(v["detail"].as_str().unwrap().contains("too many"), "{v}");
    // Rejected batch must not leave enrollment files.
    let enroll = db.data_dir.join("enroll").join(eid.to_string());
    if enroll.is_dir() {
        let n = std::fs::read_dir(&enroll).unwrap().count();
        assert_eq!(n, 0, "rejected batch must not leave files");
    }
}

#[tokio::test]
async fn enrollment_limits_corrupt_image_400_no_mutation() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let pool = state.pool.clone();
    let eid = create_employee(&pool, "L3", "Bad", None).await.unwrap();
    let router = app(state);
    let token = login_token(&router, &password).await;

    let (boundary, body) = multipart_body(&[("file", b"not-a-real-image", "x.png")]);
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/employees/{eid}/images"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let rows = list_employee_images(&pool, eid).await.unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn enrollment_missing_employee_404() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let router = app(state);
    let token = login_token(&router, &password).await;
    let png = solid_png(40, 40, 100);
    let (boundary, body) = multipart_body(&[("file", &png, "a.png")]);
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/employees/99999/images")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn enrollment_upload_and_recompute_cumulative() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let pool = state.pool.clone();
    let data_dir = state.settings.data_dir.clone();
    let eid = create_employee(&pool, "C1", "Cum", None).await.unwrap();
    let router = app(state);
    let token = login_token(&router, &password).await;

    let png_a = solid_png(64, 64, 140);
    let (boundary, body) = multipart_body(&[("file", &png_a, "a.png")]);
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/employees/{eid}/images"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["usable"], 1);
    assert_eq!(v["embedding_ready"], true);
    assert!(v.get("results").is_some());
    assert_eq!(v["gallery_reload_pending"], false);

    let png_b = solid_png(64, 64, 150);
    let (boundary, body) = multipart_body(&[("file", &png_b, "b.png")]);
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/employees/{eid}/images"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["usable"], 2, "cumulative A+B: {v}");
    assert_eq!(v["num_images_used"], 2);

    let before = list_employee_images(&pool, eid).await.unwrap();
    let ids: Vec<_> = before.iter().map(|r| r.id).collect();
    let paths: Vec<_> = before.iter().map(|r| r.file_path.clone()).collect();

    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/employees/{eid}/recompute-embedding"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["usable"], 2);
    assert_eq!(v["received"], 0);

    let after = list_employee_images(&pool, eid).await.unwrap();
    assert_eq!(after.iter().map(|r| r.id).collect::<Vec<_>>(), ids);
    assert_eq!(
        after
            .iter()
            .map(|r| r.file_path.clone())
            .collect::<Vec<_>>(),
        paths
    );
    for p in &paths {
        assert!(data_dir.join(p).is_file());
    }
}

#[tokio::test]
async fn enrollment_limits_over_dimension_400() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let dim = state.settings.max_enroll_image_dim;
    let pool = state.pool.clone();
    let eid = create_employee(&pool, "L4", "Dim", None).await.unwrap();
    let router = app(state);
    let token = login_token(&router, &password).await;

    let big = solid_png(dim + 1, 32, 100);
    let (boundary, body) = multipart_body(&[("file", &big, "big.png")]);
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/employees/{eid}/images"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(list_employee_images(&pool, eid).await.unwrap().is_empty());
}

// ── Enrollment preview analyze (guided capture) ──────────────────────────────

#[tokio::test]
async fn analyze_requires_auth() {
    let db = temp_db();
    let state = test_state(&db).await;
    let router = app(state);
    let png = solid_png(64, 64, 140);
    let (boundary, body) = multipart_body(&[("file", &png, "a.png")]);
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/enrollment/analyze")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn analyze_accepts_single_face() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let router = app(state);
    let token = login_token(&router, &password).await;
    let png = solid_png(64, 64, 140);
    let (boundary, body) = multipart_body(&[("file", &png, "face.png")]);
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/enrollment/analyze")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["accepted"], true, "{v}");
    assert_eq!(v["face_count"], 1);
    assert!(v["reason"].is_null());
    assert!(v["bbox"].is_array(), "normalized bbox required: {v}");
    let bbox = v["bbox"].as_array().unwrap();
    assert_eq!(bbox.len(), 4);
    // No embeddings over the wire
    assert!(v.get("embedding").is_none());
    assert!(v.get("embeddings").is_none());
}

#[tokio::test]
async fn analyze_rejects_no_face() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let router = app(state);
    let token = login_token(&router, &password).await;
    // near-black → test engine returns zero faces
    let png = solid_png(64, 64, 0);
    let (boundary, body) = multipart_body(&[("file", &png, "dark.png")]);
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/enrollment/analyze")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["accepted"], false);
    assert_eq!(v["reason"], "no_face");
    assert_eq!(v["face_count"], 0);
    assert!(v["bbox"].is_null());
}

#[tokio::test]
async fn analyze_invalid_payload_400() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let router = app(state);
    let token = login_token(&router, &password).await;
    let (boundary, body) = multipart_body(&[("file", b"not-an-image", "x.png")]);
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/enrollment/analyze")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn analyze_empty_batch_400() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let router = app(state);
    let token = login_token(&router, &password).await;
    let boundary = "----empty";
    let body = format!("--{boundary}--\r\n").into_bytes();
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/enrollment/analyze")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
