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
    build_daily, create_employee as db_create, daily_csv as db_daily_csv, employee_dict,
    list_cameras, list_employees as db_list_employees,
};
use pksp_vision::{enroll_images, reload_gallery};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

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
    let row = sqlx::query("SELECT id FROM employees WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    if row.is_none() {
        return Err(AppError::NotFound("Not found".into()));
    }
    if let Some(n) = body.full_name {
        sqlx::query("UPDATE employees SET full_name=? WHERE id=?")
            .bind(n)
            .bind(id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(d) = body.department {
        sqlx::query("UPDATE employees SET department=? WHERE id=?")
            .bind(d)
            .bind(id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(a) = body.is_active {
        sqlx::query("UPDATE employees SET is_active=? WHERE id=?")
            .bind(if a { 1 } else { 0 })
            .bind(id)
            .execute(&state.pool)
            .await?;
    }
    Ok(Json(employee_dict(&state.pool, id).await?))
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
    let mut files = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let name = field.file_name().unwrap_or("upload.jpg").to_string();
        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?
            .to_vec();
        files.push((name, data));
    }
    let result = enroll_images(
        &state.pool,
        &state.settings,
        state.engine.as_ref(),
        id,
        files,
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    reload_gallery(&state.pool, &state.gallery, &state.settings)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(result))
}

pub async fn recompute_embedding(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    // Re-read images from disk and recompute via empty upload path
    let rows = sqlx::query("SELECT file_path FROM employee_images WHERE employee_id=?")
        .bind(id)
        .fetch_all(&state.pool)
        .await?;
    let mut files = Vec::new();
    for r in rows {
        let rel: String = r.get("file_path");
        let path = state.settings.data_dir.join(&rel);
        if let Ok(data) = std::fs::read(&path) {
            files.push((
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("img.jpg")
                    .to_string(),
                data,
            ));
        }
    }
    // Clear and re-add would duplicate — for MVP recompute from files without re-inserting images:
    // simplify: just re-run mean on existing usable images via enroll of temp re-read without add
    // For contract: return enroll-like shape
    if files.is_empty() {
        return Ok(Json(json!({
            "received": 0,
            "usable": 0,
            "rejected": [],
            "embedding_ready": false,
            "num_images_used": 0,
        })));
    }
    // Delete image rows then re-enroll (keeps files)
    sqlx::query("DELETE FROM employee_images WHERE employee_id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    let result = enroll_images(
        &state.pool,
        &state.settings,
        state.engine.as_ref(),
        id,
        files,
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;
    reload_gallery(&state.pool, &state.gallery, &state.settings)
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
