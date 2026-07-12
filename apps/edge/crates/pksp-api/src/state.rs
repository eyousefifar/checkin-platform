use pksp_db::Settings;
use pksp_media::MediaStatus;
use pksp_vision::{FaceEngine, Gallery, VisionHandle};
use sqlx::SqlitePool;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub pool: SqlitePool,
    pub gallery: Arc<RwLock<Gallery>>,
    pub engine: Arc<dyn FaceEngine>,
    pub hub_tx: broadcast::Sender<serde_json::Value>,
    pub vision: Option<VisionHandle>,
    pub media_status: Arc<Mutex<MediaStatus>>,
}
