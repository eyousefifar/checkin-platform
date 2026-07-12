//! Employee DELETE / PATCH contracts and gallery invalidation.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use pksp_api::{app, AppState};
use pksp_core::pack_embedding;
use pksp_db::{
    bump_gallery_version, connect_pool, create_employee, gallery_version, save_embedding, Settings,
};
use pksp_media::MediaStatus;
use pksp_vision::{reload_gallery, Gallery, MockFaceEngine};
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
    let data_dir = std::env::temp_dir().join(format!("pksp-emp-contract-{id}"));
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
    settings.jwt_secret = "test-jwt-secret-for-employee-contract".into();
    settings.jwt_ttl_hours = 1;
    settings.cors_origins = vec!["http://localhost:3000".into()];
    settings.mock_vision = true;
    settings.vision_enabled = false;
    settings.require_real_vision = false;
    settings.camera_upsert = true;
    settings.cam_out_rtsp = String::new();
    settings.embedding_dim = 8;
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
    body_json(res).await["access_token"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn seed_gallery_employee(state: &AppState, code: &str, name: &str) -> i64 {
    let id = create_employee(&state.pool, code, name, Some("Ops"))
        .await
        .unwrap();
    let dim = state.settings.embedding_dim;
    let vec: Vec<f32> = (0..dim).map(|i| (i as f32 + 1.0) * 0.1).collect();
    let blob = pack_embedding(&vec, dim).unwrap();
    save_embedding(&state.pool, id, &blob, dim, 1, "test")
        .await
        .unwrap();
    // Image row so counts can be asserted after soft-delete.
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query(
        "INSERT INTO employee_images(employee_id, file_path, usable, reject_reason, created_at)
         VALUES(?,?,1,NULL,?)",
    )
    .bind(id)
    .bind(format!("enroll/{id}/fake.png"))
    .bind(&now)
    .execute(&state.pool)
    .await
    .unwrap();
    reload_gallery(&state.pool, &state.gallery, &state.settings)
        .await
        .unwrap();
    id
}

async fn count_table(pool: &sqlx::SqlitePool, table: &str) -> i64 {
    let q = format!("SELECT COUNT(*) as c FROM {table}");
    let row = sqlx::query(&q).fetch_one(pool).await.unwrap();
    use sqlx::Row;
    row.get::<i64, _>("c")
}

#[tokio::test]
async fn employee_delete_without_auth_returns_401() {
    let db = temp_db();
    let state = test_state(&db).await;
    let router = app(state);
    let res = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/employees/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn employee_delete_unknown_returns_404() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let router = app(state);
    let token = login_token(&router, &password).await;
    let res = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/employees/99999")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn employee_delete_soft_idempotent_preserves_rows_and_clears_gallery() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let pool = state.pool.clone();
    let gallery = state.gallery.clone();
    let eid = seed_gallery_employee(&state, "E-DEL", "Delete Me").await;
    assert!(
        gallery.read().unwrap().employee_ids.contains(&eid),
        "seeded into gallery"
    );
    let ver_before = gallery_version(&pool).await.unwrap();
    let emp_n = count_table(&pool, "employees").await;
    let img_n = count_table(&pool, "employee_images").await;
    let emb_n = count_table(&pool, "employee_embeddings").await;

    let router = app(state);
    let token = login_token(&router, &password).await;

    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/employees/{eid}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["is_active"], false);
    assert_eq!(body["id"], eid);
    assert_eq!(body["employee_code"], "E-DEL");

    assert_eq!(count_table(&pool, "employees").await, emp_n);
    assert_eq!(count_table(&pool, "employee_images").await, img_n);
    assert_eq!(count_table(&pool, "employee_embeddings").await, emb_n);
    assert!(
        !gallery.read().unwrap().employee_ids.contains(&eid),
        "inactive employee must leave live gallery"
    );
    let ver_after = gallery_version(&pool).await.unwrap();
    assert!(ver_after > ver_before, "version bumps on deactivation");

    // Repeat DELETE is idempotent 200, no further version bump when already inactive.
    let res = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/employees/{eid}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["is_active"], false);
    assert_eq!(
        gallery_version(&pool).await.unwrap(),
        ver_after,
        "no-op delete must not bump version"
    );
    assert_eq!(count_table(&pool, "employees").await, emp_n);
    assert_eq!(count_table(&pool, "employee_images").await, img_n);
    assert_eq!(count_table(&pool, "employee_embeddings").await, emb_n);
}

#[tokio::test]
async fn employee_patch_name_and_active_updates_gallery() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let pool = state.pool.clone();
    let gallery = state.gallery.clone();
    let eid = seed_gallery_employee(&state, "E-P", "Patch Me").await;
    let ver0 = gallery_version(&pool).await.unwrap();
    let router = app(state);
    let token = login_token(&router, &password).await;

    // Name change refreshes gallery labels and bumps version.
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/employees/{eid}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"full_name":"Patched Name"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["full_name"], "Patched Name");
    assert_eq!(body["is_active"], true);
    let ver1 = gallery_version(&pool).await.unwrap();
    assert!(ver1 > ver0);
    {
        let g = gallery.read().unwrap();
        assert!(g.employee_ids.contains(&eid));
        let idx = g.employee_ids.iter().position(|x| *x == eid).unwrap();
        assert_eq!(g.names[idx], "Patched Name");
    }

    // Department-only change does not bump gallery version.
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/employees/{eid}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"department":"Finance"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["department"], "Finance");
    assert_eq!(gallery_version(&pool).await.unwrap(), ver1);

    // Deactivate via PATCH removes from gallery.
    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/employees/{eid}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"is_active":false}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["is_active"], false);
    let ver2 = gallery_version(&pool).await.unwrap();
    assert!(ver2 > ver1);
    assert!(!gallery.read().unwrap().employee_ids.contains(&eid));

    // Reactivate restores eligibility.
    let res = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/employees/{eid}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"is_active":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["is_active"], true);
    assert!(gallery.read().unwrap().employee_ids.contains(&eid));
    assert!(gallery_version(&pool).await.unwrap() > ver2);
}

#[tokio::test]
async fn employee_patch_unknown_returns_404() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let router = app(state);
    let token = login_token(&router, &password).await;
    let res = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/employees/424242")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"full_name":"X"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn employee_unchanged_patch_does_not_bump_version() {
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let pool = state.pool.clone();
    let eid = seed_gallery_employee(&state, "E-U", "Same").await;
    let ver = gallery_version(&pool).await.unwrap();
    let router = app(state);
    let token = login_token(&router, &password).await;
    let res = router
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/employees/{eid}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"full_name":"Same"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(gallery_version(&pool).await.unwrap(), ver);
}

#[tokio::test]
async fn employee_committed_delete_returns_200_even_if_gallery_was_stale() {
    // Precondition: DB version ahead of in-memory gallery (stale).
    // Handler still returns the committed employee and reloads on success.
    let db = temp_db();
    let state = test_state(&db).await;
    let password = state.settings.admin_password.clone();
    let pool = state.pool.clone();
    let gallery = state.gallery.clone();
    let eid = seed_gallery_employee(&state, "E-R", "Reload").await;
    let ver = bump_gallery_version(&pool).await.unwrap();
    assert!(gallery.read().unwrap().employee_ids.contains(&eid));
    assert_ne!(gallery.read().unwrap().version, ver);

    let router = app(state);
    let token = login_token(&router, &password).await;
    let res = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/employees/{eid}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["is_active"], false);
    assert!(!gallery.read().unwrap().employee_ids.contains(&eid));
    assert!(gallery_version(&pool).await.unwrap() >= ver);
}
