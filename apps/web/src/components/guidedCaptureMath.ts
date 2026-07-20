/**
 * Pure helpers for guided face-capture stability: EMA smoothing, pose hysteresis,
 * soft face-in-guide scoring, and hold-to-lock progress.
 */

export type BBox = [number, number, number, number];

/** Guide frame as normalized rect (x, y, w, h) in unmirrored image space. */
export type GuideRect = {
  x: number;
  y: number;
  w: number;
  h: number;
};

/** Default guide matches the UI target: 42% × 55% centered. */
export const DEFAULT_GUIDE: GuideRect = {
  x: (1 - 0.42) / 2,
  y: (1 - 0.55) / 2,
  w: 0.42,
  h: 0.55,
};

export const BBOX_SMOOTH_ALPHA = 0.35;
export const YAW_SMOOTH_ALPHA = 0.4;
/** Degrees of expansion applied once a slot is sticky (entered). */
export const YAW_HOLD_PAD_DEG = 2.5;
/** Continuous good window before capture. */
export const STABLE_HOLD_MS = 800;
/** Face-center distance from guide center (normalized frame coords). */
export const GUIDE_CENTER_TOL = 0.18;
/** Face height relative to guide height — wide band for easy framing. */
export const GUIDE_SIZE_MIN = 0.55;
export const GUIDE_SIZE_MAX = 1.35;
/** Minimum sticky window for status text to avoid thrash. */
export const STATUS_STICKY_MS = 300;

export type PoseYawRange = {
  id: string;
  yawMin: number;
  yawMax: number;
};

/**
 * Exponential moving average on a normalized bbox.
 * Seeds from `next` when `prev` is null (first detection / after face lost).
 */
export function smoothBbox(
  prev: BBox | null,
  next: BBox | null,
  alpha: number = BBOX_SMOOTH_ALPHA,
): BBox | null {
  if (!next) return null;
  if (!prev) return next;
  const a = clamp01(alpha);
  return [
    prev[0] + (next[0] - prev[0]) * a,
    prev[1] + (next[1] - prev[1]) * a,
    prev[2] + (next[2] - prev[2]) * a,
    prev[3] + (next[3] - prev[3]) * a,
  ];
}

/**
 * EMA on signed yaw. Resets when `next` is null (no landmarks / no face).
 */
export function smoothYaw(
  prev: number | null,
  next: number | null,
  alpha: number = YAW_SMOOTH_ALPHA,
): number | null {
  if (next == null) return null;
  if (prev == null) return next;
  const a = clamp01(alpha);
  return prev + (next - prev) * a;
}

/**
 * Pose bin match with optional hysteresis for the active/sticky slot.
 *
 * - Entry: use published yawMin/yawMax
 * - Hold (sticky): expand by `holdPad` so small noise does not drop the lock
 * - Null yaw: only center is acceptable (model treats as frontal)
 */
export function yawMatchesSlot(
  yaw: number | null,
  slot: PoseYawRange,
  opts?: { sticky?: boolean; holdPad?: number },
): boolean {
  if (yaw == null) {
    return slot.id === "center";
  }
  const pad = opts?.sticky ? (opts.holdPad ?? YAW_HOLD_PAD_DEG) : 0;
  return yaw >= slot.yawMin - pad && yaw <= slot.yawMax + pad;
}

export type GuideScore = {
  ok: boolean;
  /** Short operator guidance when not ok; null when framed well. */
  hint: string | null;
};

/**
 * Soft face-in-guide check. Generous tolerances — not a tight bullseye.
 * Bbox is normalized xyxy in unmirrored image space (same as server).
 */
export function faceGuideScore(
  bbox: BBox | null,
  guide: GuideRect = DEFAULT_GUIDE,
  opts?: {
    centerTol?: number;
    sizeMin?: number;
    sizeMax?: number;
  },
): GuideScore {
  if (!bbox) {
    return { ok: false, hint: "Center your face in the target" };
  }
  const centerTol = opts?.centerTol ?? GUIDE_CENTER_TOL;
  const sizeMin = opts?.sizeMin ?? GUIDE_SIZE_MIN;
  const sizeMax = opts?.sizeMax ?? GUIDE_SIZE_MAX;

  const [x1, y1, x2, y2] = bbox;
  const faceCx = (x1 + x2) / 2;
  const faceCy = (y1 + y2) / 2;
  const faceH = Math.max(0, y2 - y1);
  const guideCx = guide.x + guide.w / 2;
  const guideCy = guide.y + guide.h / 2;

  const sizeRatio = guide.h > 0 ? faceH / guide.h : 0;
  if (sizeRatio < sizeMin) {
    return { ok: false, hint: "Move closer — face is small in frame" };
  }
  if (sizeRatio > sizeMax) {
    return { ok: false, hint: "Move back slightly" };
  }

  const dx = faceCx - guideCx;
  const dy = faceCy - guideCy;
  if (Math.abs(dx) > centerTol || Math.abs(dy) > centerTol) {
    // Prefer the dominant axis for a single short hint.
    if (Math.abs(dx) >= Math.abs(dy)) {
      // Server bbox is unmirrored; UI video is CSS-mirrored (selfie).
      // Face left in unmirrored space appears on the right of the preview, so
      // the operator must move left to recenter (and vice versa).
      return {
        ok: false,
        hint: dx < 0 ? "Move left a little" : "Move right a little",
      };
    }
    return {
      ok: false,
      hint: dy < 0 ? "Move down a little" : "Move up a little",
    };
  }

  return { ok: true, hint: null };
}

export type StabilityState = {
  /** Timestamp when continuous-good streak started, or null. */
  goodSince: number | null;
  /** 0–1 progress toward lock. */
  progress: number;
  /** True when hold window completed. */
  locked: boolean;
};

export function idleStability(): StabilityState {
  return { goodSince: null, progress: 0, locked: false };
}

/**
 * Advance hold-to-lock state. `good` means all capture preconditions hold this tick.
 */
export function advanceStability(
  prev: StabilityState,
  good: boolean,
  now: number,
  holdMs: number = STABLE_HOLD_MS,
): StabilityState {
  if (!good) {
    return idleStability();
  }
  const goodSince = prev.goodSince ?? now;
  const elapsed = Math.max(0, now - goodSince);
  const progress = holdMs > 0 ? Math.min(1, elapsed / holdMs) : 1;
  return {
    goodSince,
    progress,
    locked: progress >= 1,
  };
}

/**
 * Debounce status text: keep previous message unless category changed or
 * sticky window elapsed.
 */
export function shouldUpdateStatus(
  prevText: string,
  nextText: string,
  lastChangedAt: number,
  now: number,
  stickyMs: number = STATUS_STICKY_MS,
): boolean {
  if (prevText === nextText) return false;
  if (statusCategory(prevText) !== statusCategory(nextText)) return true;
  return now - lastChangedAt >= stickyMs;
}

function statusCategory(text: string): string {
  const t = text.toLowerCase();
  if (t.includes("no face") || t.includes("center your face")) return "no_face";
  if (t.includes("multiple")) return "multi";
  if (t.includes("move closer") || t.includes("too small")) return "closer";
  if (t.includes("move back")) return "back";
  if (t.includes("move left") || t.includes("move right") || t.includes("move up") || t.includes("move down"))
    return "position";
  if (t.includes("blur") || t.includes("hold still")) return "blur";
  if (t.includes("light") || t.includes("glare") || t.includes("dark") || t.includes("bright"))
    return "light";
  if (t.includes("locking") || t.includes("hold steady") || t.includes("hold…")) return "locking";
  if (t.includes("captured") || t.includes("lock") || t.includes("acquired")) return "done";
  if (t.includes("turn") || t.includes("face the camera") || t.includes("nose")) return "pose";
  if (t.includes("yaw") || t.includes("extreme")) return "pose";
  return t.slice(0, 24);
}

function clamp01(v: number): number {
  if (!Number.isFinite(v)) return 0;
  return Math.min(1, Math.max(0, v));
}
