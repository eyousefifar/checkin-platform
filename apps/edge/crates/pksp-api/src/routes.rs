use crate::auth::{create_token, password_ok, verify_token, AuthUser};
use crate::error::AppError;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::{SinkExt, StreamExt};
use pksp_db::{
    build_daily, create_employee as db_create, daily_csv as db_daily_csv,
    deactivate_employee as db_deactivate, employee_dict, list_cameras,
    list_employees as db_list_employees, update_employee_fields, EmployeePatch,
};
use pksp_vision::{enroll_images, reload_gallery};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::error;

/// Rate-limit gallery reload error logs (monotonic seconds of last emit).
static GALLERY_RELOAD_ERR_LOG_SEC: AtomicU64 = AtomicU64::new(0);

/// One immediate retry after commit; never fails the HTTP response.
async fn converge_gallery_after_commit(state: &AppState) {
    if reload_gallery(&state.pool, &state.gallery, &state.settings)
        .await
        .is_ok()
    {
        return;
    }
    if let Err(e2) = reload_gallery(&state.pool, &state.gallery, &state.settings).await {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let prev = GALLERY_RELOAD_ERR_LOG_SEC.load(Ordering::Relaxed);
        if now.saturating_sub(prev) >= 30 {
            GALLERY_RELOAD_ERR_LOG_SEC.store(now, Ordering::Relaxed);
            error!(
                error = %e2,
                "gallery reload failed after employee mutation; version poll will converge"
            );
        }
    }
}

pub async fn health(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let cams = list_cameras(&state.pool, true).await?;
    let media = state.media_status.lock().await;
    let g = state.gallery.read().unwrap();
    let cameras: Vec<Value> = cams
        .into_iter()
        .map(|c| {
            let path = if c.id == "cam_in" {
                // Browser-safe path: explicit H.264 path, transcoder publish, or configured
                if !state.settings.cam_in_h264_rtsp.is_empty() {
                    // Native H.264 camera still uses configured webrtc path (often cam_in)
                    c.webrtc_path.clone()
                } else if let Some(p) = &media.preferred_webrtc_path {
                    p.clone()
                } else if c.webrtc_path == "cam_in"
                    && (state.settings.cam_in_rtsp.contains("stream1")
                        || std::env::var("FORCE_TRANSCODE").as_deref() == Ok("true"))
                {
                    // Prefer H.264 publish path when we expect H.265 source
                    "cam_in_h264".to_string()
                } else {
                    c.webrtc_path.clone()
                }
            } else {
                c.webrtc_path.clone()
            };
            json!({
                "id": c.id,
                "name": c.name,
                "direction": c.direction,
                "enabled": c.enabled,
                "webrtc_path": path,
            })
        })
        .collect();
    Ok(Json(json!({
        "status": "ok",
        "vision_ready": state.engine.ready(),
        "vision_provider": state.engine.execution_provider(),
        "gallery_size": g.size(),
        "cameras": cameras,
        "media": {
            "mediamtx_running": media.mediamtx_running,
            "transcoder_running": media.transcoder_running,
            "last_error": media.last_error,
            "mediamtx_path": media.mediamtx_path,
            "ffmpeg_path": media.ffmpeg_path,
        }
    })))
}

#[derive(Deserialize)]
pub struct LoginBody {
    password: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<Json<Value>, AppError> {
    if !password_ok(&state.settings.admin_password, &body.password) {
        return Err(AppError::Unauthorized("Invalid password".into()));
    }
    let token = create_token(&state.settings.jwt_secret, state.settings.jwt_ttl_hours)?;
    Ok(Json(json!({
        "access_token": token,
        "token_type": "bearer",
    })))
}

#[derive(Deserialize)]
pub struct ListQuery {
    q: Option<String>,
}

pub async fn list_employees(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let rows = db_list_employees(&state.pool, q.q.as_deref()).await?;
    Ok(Json(Value::Array(rows)))
}

#[derive(Deserialize)]
pub struct EmployeeCreate {
    employee_code: String,
    full_name: String,
    department: Option<String>,
}

pub async fn create_employee(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(body): Json<EmployeeCreate>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM employees WHERE employee_code = ?")
        .bind(&body.employee_code)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Employee code already exists".into()));
    }
    let id = db_create(
        &state.pool,
        &body.employee_code,
        &body.full_name,
        body.department.as_deref(),
    )
    .await?;
    let dict = employee_dict(&state.pool, id).await?;
    Ok((StatusCode::CREATED, Json(dict)))
}

pub async fn get_employee(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    match employee_dict(&state.pool, id).await {
        Ok(d) => Ok(Json(d)),
        Err(_) => Err(AppError::NotFound("Not found".into())),
    }
}

#[derive(Deserialize)]
pub struct EmployeeUpdate {
    full_name: Option<String>,
    department: Option<String>,
    is_active: Option<bool>,
}

pub async fn update_employee(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<EmployeeUpdate>,
) -> Result<Json<Value>, AppError> {
    let touches_gallery = body.full_name.is_some() || body.is_active.is_some();
    let outcome = update_employee_fields(
        &state.pool,
        id,
        EmployeePatch {
            full_name: body.full_name,
            department: body.department,
            is_active: body.is_active,
        },
    )
    .await?
    .ok_or_else(|| AppError::NotFound("Not found".into()))?;

    // Converge in-memory gallery after name/active mutations (including no-ops).
    if touches_gallery {
        converge_gallery_after_commit(&state).await;
    }
    Ok(Json(outcome.employee))
}

/// Soft-delete: set `is_active=false`. Idempotent; preserves rows and files.
pub async fn deactivate_employee(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let outcome = db_deactivate(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Not found".into()))?;
    // Always attempt gallery convergence (even when already inactive).
    converge_gallery_after_commit(&state).await;
    Ok(Json(outcome.employee))
}

pub async fn upload_images(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<Value>, AppError> {
    let exists = sqlx::query("SELECT id FROM employees WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Not found".into()));
    }

    let max_files = state.settings.max_enroll_files;
    let max_file = state.settings.max_enroll_file_bytes;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        // Accept only file parts (multipart file fields have a file_name).
        let Some(name) = field.file_name().map(|s| s.to_string()) else {
            return Err(AppError::BadRequest("only file fields are accepted".into()));
        };
        if files.len() >= max_files {
            return Err(AppError::BadRequest(format!(
                "too many files; max is {max_files}"
            )));
        }
        // Chunked read — stop before exceeding per-file limit (do not field.bytes() first).
        let mut data = Vec::new();
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    if data.len().saturating_add(chunk.len()) > max_file {
                        return Err(AppError::BadRequest(format!(
                            "file exceeds max size of {max_file} bytes"
                        )));
                    }
                    data.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(AppError::BadRequest(e.to_string())),
            }
        }
        if data.is_empty() {
            return Err(AppError::BadRequest("empty image".into()));
        }
        pksp_vision::validate_enroll_image_bytes(&data, &state.settings)
            .map_err(AppError::BadRequest)?;
        files.push((name, data));
    }

    if files.is_empty() {
        return Err(AppError::BadRequest("no images provided".into()));
    }

    let result = enroll_images(
        &state.pool,
        &state.settings,
        state.engine.clone(),
        id,
        files,
        Some(&state.gallery),
    )
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if let Some(rest) = msg.strip_prefix("validation: ") {
            AppError::BadRequest(rest.to_string())
        } else {
            AppError::Internal(msg)
        }
    })?;
    Ok(Json(result))
}

pub async fn recompute_embedding(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let exists = sqlx::query("SELECT id FROM employees WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Not found".into()));
    }

    // Zero new files — reanalyze existing rows in place (preserve ids/paths).
    let result = enroll_images(
        &state.pool,
        &state.settings,
        state.engine.clone(),
        id,
        Vec::new(),
        Some(&state.gallery),
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct DateQuery {
    date: Option<String>,
    employee_id: Option<i64>,
}

pub async fn daily(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<DateQuery>,
) -> Result<Json<Value>, AppError> {
    let day = q
        .date
        .unwrap_or_else(|| chrono::Utc::now().date_naive().to_string());
    let rows = build_daily(&state.pool, &day).await?;
    Ok(Json(Value::Array(rows)))
}

pub async fn daily_csv(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<DateQuery>,
) -> Result<Response, AppError> {
    let day = q
        .date
        .unwrap_or_else(|| chrono::Utc::now().date_naive().to_string());
    let content = db_daily_csv(&state.pool, &day).await?;
    let mut res = Response::new(Body::from(content));
    *res.status_mut() = StatusCode::OK;
    res.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        "text/csv; charset=utf-8".parse().unwrap(),
    );
    res.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"attendance-{day}.csv\"")
            .parse()
            .unwrap(),
    );
    Ok(res)
}

pub async fn events(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<DateQuery>,
) -> Result<Json<Value>, AppError> {
    let mut sql = String::from(
        "SELECT id, employee_id, camera_id, kind, score, ts, local_date FROM attendance_events WHERE 1=1",
    );
    if q.date.is_some() {
        sql.push_str(" AND local_date = ?");
    }
    if q.employee_id.is_some() {
        sql.push_str(" AND employee_id = ?");
    }
    sql.push_str(" ORDER BY ts DESC LIMIT 500");
    let mut query = sqlx::query(&sql);
    if let Some(d) = &q.date {
        query = query.bind(d);
    }
    if let Some(e) = q.employee_id {
        query = query.bind(e);
    }
    let rows = query.fetch_all(&state.pool).await?;
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            let ts: String = r.get("ts");
            json!({
                "id": r.get::<i64,_>("id"),
                "employee_id": r.get::<Option<i64>,_>("employee_id"),
                "camera_id": r.get::<String,_>("camera_id"),
                "kind": r.get::<String,_>("kind"),
                "score": r.get::<Option<f64>,_>("score"),
                "ts": format!("{ts}Z"),
                "local_date": r.get::<String,_>("local_date"),
            })
        })
        .collect();
    Ok(Json(Value::Array(out)))
}

pub async fn list_cameras_route(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let cams = list_cameras(&state.pool, false).await?;
    let online: HashMap<String, bool> = state
        .vision
        .as_ref()
        .map(|v| v.metrics.read().unwrap().online.clone())
        .unwrap_or_default();
    let out: Vec<Value> = cams
        .into_iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "direction": c.direction,
                "enabled": c.enabled,
                "webrtc_path": c.webrtc_path,
                "rtsp_url": c.rtsp_url,
                "online": online.get(&c.id).copied().unwrap_or(false),
            })
        })
        .collect();
    Ok(Json(Value::Array(out)))
}

#[derive(Deserialize)]
pub struct WsQuery {
    token: Option<String>,
}

pub async fn ws_live(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(token) = q.token {
        verify_token(&state.settings.jwt_secret, &token)?;
    }
    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state)))
}

async fn handle_ws(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.hub_tx.subscribe();
    let version = state.gallery.read().unwrap().version;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let hello = json!({
        "type": "hello",
        "server_ts": ts,
        "gallery_version": version,
    });
    let _ = sender.send(Message::Text(hello.to_string().into())).await;

    let send_task = tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if sender
                .send(Message::Text(ev.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(t) = msg {
            if let Ok(v) = serde_json::from_str::<Value>(&t) {
                if v.get("type").and_then(|t| t.as_str()) == Some("ping") {
                    // ignore — client pings
                }
            }
        } else if matches!(msg, Message::Close(_)) {
            break;
        }
    }
    send_task.abort();
}
