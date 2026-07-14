//! Live WebSocket transport tests — real loopback listener + tungstenite client.

mod common;

use futures_util::StreamExt;
use pksp_api::{app, AppState};
use pksp_db::{connect_pool, Settings};
use pksp_media::MediaStatus;
use pksp_vision::Gallery;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
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
    let data_dir = std::env::temp_dir().join(format!("pksp-ws-test-{id}"));
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
    settings.jwt_secret = "test-jwt-secret-for-websocket".into();
    settings.jwt_ttl_hours = 1;
    settings.cors_origins = vec!["http://localhost:3000".into()];
    settings.vision_enabled = false;
    settings.camera_upsert = true;
    settings.cam_out_rtsp = String::new();
    settings
}

async fn test_state_with_capacity(db: &TestDb, capacity: usize) -> AppState {
    let settings = Arc::new(test_settings(db));
    let pool = connect_pool(&settings).await.expect("connect_pool");
    let engine = common::test_engine(settings.embedding_dim);
    let gallery = Arc::new(RwLock::new(Gallery::empty(
        settings.match_threshold,
        settings.match_margin,
    )));
    let (hub_tx, _) = tokio::sync::broadcast::channel(capacity);
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

struct LiveServer {
    url: String,
    cancel: tokio::sync::watch::Sender<bool>,
    join: tokio::task::JoinHandle<()>,
}

impl LiveServer {
    async fn start(state: AppState) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let app = app(state);
        let join = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = cancel_rx.wait_for(|v| *v).await;
                })
                .await
                .ok();
        });
        // Brief yield so accept loop is ready.
        tokio::task::yield_now().await;
        LiveServer {
            url: format!("ws://{addr}/api/ws/live"),
            cancel: cancel_tx,
            join,
        }
    }

    async fn shutdown(self) {
        let _ = self.cancel.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(2), self.join).await;
    }
}

async fn read_text(
    ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
) -> Option<String> {
    loop {
        match tokio::time::timeout(Duration::from_secs(2), ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => return Some(t.to_string()),
            Ok(Some(Ok(Message::Ping(p)))) => {
                // ignore; client lib may handle
                let _ = p;
                continue;
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) => return None,
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(_))) | Err(_) => return None,
        }
    }
}

#[tokio::test]
async fn websocket_hello_then_event() {
    let db = temp_db();
    let state = test_state_with_capacity(&db, 16).await;
    let hub = state.hub_tx.clone();
    let server = LiveServer::start(state).await;

    let (mut ws, _) = tokio_tungstenite::connect_async(&server.url)
        .await
        .expect("connect");
    let hello = read_text(&mut ws).await.expect("hello");
    let v: Value = serde_json::from_str(&hello).unwrap();
    assert_eq!(v["type"], "hello");

    hub.send(json!({"type": "metrics", "cameras_online": 1, "marker": "a"}))
        .unwrap();
    let msg = read_text(&mut ws).await.expect("event");
    let v: Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(v["type"], "metrics");
    assert_eq!(v["marker"], "a");

    let _ = ws.close(None).await;
    server.shutdown().await;
}

#[tokio::test]
async fn websocket_lagged_receiver_continues_or_closes() {
    let db = temp_db();
    // Tiny buffer so a burst overruns the subscriber.
    let state = test_state_with_capacity(&db, 2).await;
    let hub = state.hub_tx.clone();
    let server = LiveServer::start(state).await;

    let (mut ws, _) = tokio_tungstenite::connect_async(&server.url)
        .await
        .expect("connect");
    let hello = read_text(&mut ws).await.expect("hello");
    assert!(hello.contains("hello"));

    // Flood without reading so the broadcast subscriber lags.
    for i in 0..64 {
        let _ = hub.send(json!({"type": "metrics", "n": i}));
    }
    // Distinctive latest event the client should still be able to receive after Lagged.
    let _ = hub.send(json!({"type": "metrics", "marker": "after-lag", "n": 999}));

    let mut saw_after = false;
    let mut closed = false;
    for _ in 0..32 {
        match tokio::time::timeout(Duration::from_millis(200), ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                if t.contains("after-lag") {
                    saw_after = true;
                    break;
                }
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) => {
                closed = true;
                break;
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(_))) => {
                closed = true;
                break;
            }
            Err(_) => {
                // timeout — keep trying a few more
                continue;
            }
        }
    }

    assert!(
        saw_after || closed,
        "lagged socket must deliver a later event or close — never stay open and silent forever"
    );

    // If still open, close cleanly.
    if !closed {
        let _ = ws.close(None).await;
    }
    server.shutdown().await;
}

#[tokio::test]
async fn websocket_closed_channel_ends_send_loop() {
    let db = temp_db();
    let state = test_state_with_capacity(&db, 8).await;
    // Close the client and ensure the server task completes shutdown cleanly.
    let server = LiveServer::start(state).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&server.url)
        .await
        .expect("connect");
    let _ = read_text(&mut ws).await;
    let _ = ws.close(None).await;
    // Drain until closed.
    let mut closed = false;
    for _ in 0..20 {
        match tokio::time::timeout(Duration::from_millis(100), ws.next()).await {
            Ok(None) | Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_))) => {
                closed = true;
                break;
            }
            _ => continue,
        }
    }
    assert!(closed, "client close must complete");
    server.shutdown().await;
}
