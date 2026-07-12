export type FaceDet = {
  track_id: number;
  bbox: [number, number, number, number];
  label: string;
  employee_id?: number | null;
  score: number;
  quality_ok: boolean;
  state: string;
};

export type DetectionsMsg = {
  type: "detections";
  camera_id: string;
  ts: number;
  frame_w: number;
  frame_h: number;
  faces: FaceDet[];
};

export type AttendanceMsg = {
  type: "attendance";
  event_id: number;
  employee_id: number;
  name: string;
  kind: string;
  camera_id: string;
  score: number;
  ts: number;
};

export type MetricsMsg = {
  type: "metrics";
  cameras_online: number;
  present_count: number;
  events_today: number;
  vision_fps: Record<string, number>;
};

export type CameraStatusMsg = {
  type: "camera_status";
  camera_id: string;
  online: boolean;
};

export type Employee = {
  id: number;
  employee_code: string;
  full_name: string;
  department: string | null;
  is_active: boolean;
  image_count: number;
  usable_images: number;
  embedding_ready: boolean;
  num_images_used: number;
  images?: {
    id: number;
    file_path: string;
    usable: boolean;
    reject_reason: string | null;
  }[];
};

/**
 * Shared enrollment upload / recompute response.
 * Frozen aggregate fields from plan 006 plus additive per-file results.
 */
export type EnrollmentResult = {
  received: number;
  usable: number;
  rejected: { filename: string; reason: string }[];
  embedding_ready: boolean;
  num_images_used: number;
  results?: { filename: string; usable: boolean; reason: string | null }[];
  gallery_reload_pending?: boolean;
};

/** @deprecated Prefer EnrollmentResult — same shape. */
export type EnrollUploadResult = EnrollmentResult;

export type DailyRow = {
  employee_id: number;
  employee_code: string;
  full_name: string;
  department: string | null;
  first_in: string | null;
  last_out: string | null;
  duration_minutes: number | null;
  status: string;
  check_in_count: number;
  check_out_count: number;
};

/** Raw row from authenticated GET `/api/attendance/events` (no nested objects). */
export type RawAttendanceEvent = {
  id: number;
  employee_id: number | null;
  camera_id: string;
  kind: string;
  score: number | null;
  /** UTC ISO timestamp (API appends Z). */
  ts: string;
  local_date: string;
};

/** Camera row from public `/api/health` (no source URLs). */
export type HealthCamera = {
  id: string;
  name: string;
  direction: string;
  enabled: boolean;
  webrtc_path: string;
};

/** Public process health — timezone is the only settings surface. */
export type HealthResponse = {
  status: string;
  timezone: string;
  vision_ready: boolean;
  vision_provider: string;
  gallery_size: number;
  cameras: HealthCamera[];
  media: {
    mediamtx_running: boolean;
    transcoder_running: boolean;
    publication: string;
    source_mode: string | null;
    preferred_webrtc_path: string | null;
    last_error: string | null;
    mediamtx_path?: string | null;
    ffmpeg_path?: string | null;
  };
};
