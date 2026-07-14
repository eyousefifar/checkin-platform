use pksp_api::serve;
use pksp_db::Settings;
use uuid::Uuid;

#[tokio::test]
async fn missing_models_fail_before_database_or_listener_startup() {
    let data_dir = std::env::temp_dir().join(format!("pksp-missing-models-{}", Uuid::new_v4()));
    let db_path = data_dir.join("must-not-exist.db");
    let abs = db_path
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();
    let mut settings = Settings::from_env();
    settings.data_dir = data_dir.clone();
    settings.database_url = format!("sqlite:////{abs}?mode=rwc");
    settings.bind_addr = "127.0.0.1:0".into();
    settings.cam_in_rtsp = "rtsp://127.0.0.1:8554/cam".into();

    let error = serve(settings).await.expect_err("missing models must fail");
    assert!(error.to_string().contains("buffalo_l ONNX"), "{error}");
    assert!(
        !db_path.exists(),
        "model validation must precede database creation"
    );
    let _ = std::fs::remove_dir_all(data_dir);
}
