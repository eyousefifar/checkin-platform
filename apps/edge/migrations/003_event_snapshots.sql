-- Additive event snapshot provenance (best-effort after commit).
-- Paths are relative to DATA_DIR; never store absolute caller-controlled paths.
ALTER TABLE attendance_events ADD COLUMN snapshot_path TEXT;
ALTER TABLE attendance_events ADD COLUMN snapshot_bbox_json TEXT;
