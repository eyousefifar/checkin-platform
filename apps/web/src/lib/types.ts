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
