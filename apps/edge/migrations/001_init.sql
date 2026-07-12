CREATE TABLE IF NOT EXISTS employees (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    employee_code TEXT NOT NULL UNIQUE,
    full_name TEXT NOT NULL,
    department TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS employee_images (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    employee_id INTEGER NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    usable INTEGER NOT NULL DEFAULT 0,
    reject_reason TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS employee_embeddings (
    employee_id INTEGER PRIMARY KEY REFERENCES employees(id) ON DELETE CASCADE,
    dim INTEGER NOT NULL DEFAULT 512,
    vector BLOB NOT NULL,
    num_images_used INTEGER NOT NULL DEFAULT 0,
    model_name TEXT NOT NULL DEFAULT 'buffalo_l',
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cameras (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    rtsp_url TEXT NOT NULL DEFAULT '',
    webrtc_path TEXT NOT NULL DEFAULT '',
    direction TEXT NOT NULL DEFAULT 'in',
    enabled INTEGER NOT NULL DEFAULT 1,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS attendance_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    employee_id INTEGER REFERENCES employees(id),
    camera_id TEXT NOT NULL REFERENCES cameras(id),
    kind TEXT NOT NULL,
    score REAL,
    margin REAL,
    track_id INTEGER,
    needs_review INTEGER NOT NULL DEFAULT 0,
    meta_json TEXT,
    ts TEXT NOT NULL,
    local_date TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_events_local_date_employee ON attendance_events(local_date, employee_id);
CREATE INDEX IF NOT EXISTS ix_events_ts ON attendance_events(ts);
CREATE INDEX IF NOT EXISTS ix_events_emp_cam_ts ON attendance_events(employee_id, camera_id, ts);

CREATE TABLE IF NOT EXISTS app_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT ''
);
