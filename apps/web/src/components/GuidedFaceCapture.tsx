"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { analyzeEnrollmentFrame } from "@/lib/api";
import type { EnrollmentAnalyzeResult, PoseSlotId } from "@/lib/types";

/**
 * Outer |yaw| for guided bins. Must stay strictly under the server default
 * POSE_MAX_YAW (30°) so every captured frame can pass enrollment quality.
 */
export const POSE_BIN_LIMIT = 28;

export const POSE_SLOTS: {
  id: PoseSlotId;
  label: string;
  /** Inclusive yaw range in degrees (signed). */
  yawMin: number;
  yawMax: number;
  prompt: string;
}[] = [
  {
    id: "center",
    label: "CENTER",
    yawMin: -6,
    yawMax: 6,
    prompt: "Face the camera · hold steady",
  },
  {
    id: "slight_left",
    label: "S-LEFT",
    yawMin: 6,
    yawMax: 16,
    prompt: "Turn slightly left · nose toward left edge",
  },
  {
    id: "left",
    label: "LEFT",
    yawMin: 16,
    yawMax: POSE_BIN_LIMIT,
    prompt: "Turn further left · stay inside pose limit",
  },
  {
    id: "slight_right",
    label: "S-RIGHT",
    yawMin: -16,
    yawMax: -6,
    prompt: "Turn slightly right · nose toward right edge",
  },
  {
    id: "right",
    label: "RIGHT",
    yawMin: -POSE_BIN_LIMIT,
    yawMax: -16,
    prompt: "Turn further right · stay inside pose limit",
  },
];

const PREVIEW_INTERVAL_MS = 450;
const CAPTURE_COOLDOWN_MS = 900;
const JPEG_QUALITY = 0.88;

export type CapturedPose = {
  slot: PoseSlotId;
  file: File;
  url: string;
};

type Props = {
  /** Called whenever the set of captured files changes. */
  onCapturedChange: (files: File[]) => void;
  disabled?: boolean;
};

function reasonGuidance(reason: string | null | undefined): string {
  switch (reason) {
    case "no_face":
      return "No face detected — center your face in the target";
    case "multiple_faces":
      return "Multiple faces — only one person in frame";
    case "face_too_small":
      return "Move closer — face is too small";
    case "low_det_score":
      return "Face unclear — improve lighting";
    case "low_blur":
      return "Image too blurry — hold still";
    case "low_pose":
      return "Yaw too extreme — ease toward center";
    case "low_light":
      return "Too dark — add light";
    case "high_glare":
      return "Too bright — reduce glare";
    default:
      return reason ? `Rejected: ${reason}` : "Analyzing…";
  }
}

function yawMatchesSlot(
  yaw: number | null,
  slot: (typeof POSE_SLOTS)[number],
): boolean {
  // Without landmarks, only center can be accepted (model treats as frontal).
  if (yaw == null) {
    return slot.id === "center";
  }
  return yaw >= slot.yawMin && yaw <= slot.yawMax;
}

export function GuidedFaceCapture({ onCapturedChange, disabled }: Props) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const inFlightRef = useRef(false);
  const abortRef = useRef<AbortController | null>(null);
  const cameraRequestRef = useRef(0);
  const lastCaptureRef = useRef(0);
  const capturedRef = useRef<CapturedPose[]>([]);

  const [cameraState, setCameraState] = useState<
    "idle" | "starting" | "live" | "denied" | "error"
  >("idle");
  const [cameraError, setCameraError] = useState("");
  const [preview, setPreview] = useState<EnrollmentAnalyzeResult | null>(null);
  const [statusText, setStatusText] = useState("Start camera to begin guided capture");
  const [captured, setCaptured] = useState<CapturedPose[]>([]);
  const [activeSlotIndex, setActiveSlotIndex] = useState(0);
  const [videoAspect, setVideoAspect] = useState(4 / 3);

  const stopCamera = useCallback(() => {
    cameraRequestRef.current += 1;
    abortRef.current?.abort();
    abortRef.current = null;
    inFlightRef.current = false;
    const stream = streamRef.current;
    if (stream) {
      stream.getTracks().forEach((t) => t.stop());
      streamRef.current = null;
    }
    if (videoRef.current) {
      videoRef.current.srcObject = null;
    }
    setCameraState((s) => (s === "live" || s === "starting" ? "idle" : s));
  }, []);

  // Cleanup on unmount: stop stream + revoke object URLs.
  useEffect(() => {
    return () => {
      stopCamera();
      capturedRef.current.forEach((c) => URL.revokeObjectURL(c.url));
    };
  }, [stopCamera]);

  // A parent transition (saving/created/navigation) must immediately release
  // camera hardware rather than merely pausing analysis.
  useEffect(() => {
    if (disabled) stopCamera();
  }, [disabled, stopCamera]);

  // Automatic capture is complete; release the camera without requiring a
  // second operator action.
  useEffect(() => {
    if (
      activeSlotIndex >= POSE_SLOTS.length &&
      captured.length === POSE_SLOTS.length &&
      streamRef.current
    ) {
      stopCamera();
    }
  }, [activeSlotIndex, captured.length, stopCamera]);

  const emitFiles = useCallback(
    (list: CapturedPose[]) => {
      onCapturedChange(list.map((c) => c.file));
    },
    [onCapturedChange],
  );

  const setCapturedAndEmit = useCallback(
    (next: CapturedPose[]) => {
      capturedRef.current = next;
      setCaptured(next);
      emitFiles(next);
      // Advance active slot to first missing
      const filled = new Set(next.map((c) => c.slot));
      const idx = POSE_SLOTS.findIndex((s) => !filled.has(s.id));
      setActiveSlotIndex(idx === -1 ? POSE_SLOTS.length : idx);
    },
    [emitFiles],
  );

  const syncVideoAspect = useCallback(() => {
    const video = videoRef.current;
    if (video && video.videoWidth > 0 && video.videoHeight > 0) {
      setVideoAspect(video.videoWidth / video.videoHeight);
    }
  }, []);

  const startCamera = useCallback(async () => {
    if (disabled) return;
    const request = ++cameraRequestRef.current;
    setCameraError("");
    setPreview(null);
    setCameraState("starting");
    setStatusText("Requesting camera…");
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        video: {
          facingMode: "user",
          width: { ideal: 1280 },
          height: { ideal: 720 },
        },
        audio: false,
      });
      if (request !== cameraRequestRef.current) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }
      streamRef.current = stream;
      const video = videoRef.current;
      if (video) {
        video.srcObject = stream;
        syncVideoAspect();
        await video.play().catch(() => {
          /* autoplay may require user gesture; track is still live */
        });
      }
      if (request !== cameraRequestRef.current) {
        stream.getTracks().forEach((track) => track.stop());
        if (video?.srcObject === stream) video.srcObject = null;
        return;
      }
      syncVideoAspect();
      setCameraState("live");
      setStatusText("Camera live · align face with target");
    } catch (err) {
      if (request !== cameraRequestRef.current) return;
      const name = err instanceof DOMException ? err.name : "";
      if (
        name === "NotAllowedError" ||
        name === "PermissionDeniedError" ||
        /denied|permission/i.test(err instanceof Error ? err.message : "")
      ) {
        setCameraState("denied");
        setCameraError(
          "Camera permission denied. Allow camera access or use manual file upload below.",
        );
        setStatusText("Camera permission denied");
      } else {
        setCameraState("error");
        setCameraError(
          err instanceof Error ? err.message : "Failed to open camera",
        );
        setStatusText("Camera error");
      }
    }
  }, [disabled, syncVideoAspect]);

  const grabJpegBlob = useCallback(async (): Promise<Blob | null> => {
    const video = videoRef.current;
    const canvas = canvasRef.current;
    if (!video || !canvas) return null;
    // readyState may be 0 in jsdom even when a stream is attached.
    const w = video.videoWidth || 640;
    const h = video.videoHeight || 480;
    if (w < 1 || h < 1) return null;
    canvas.width = w;
    canvas.height = h;
    try {
      const ctx = canvas.getContext("2d");
      if (ctx) {
        // Mirror-consistent: draw as-is (model coords unmirrored; CSS may mirror display).
        ctx.drawImage(video, 0, 0, w, h);
      }
    } catch {
      /* jsdom may lack full canvas support — still emit a JPEG blob for preview */
    }
    return new Promise((resolve) => {
      try {
        canvas.toBlob(
          (b) => resolve(b ?? new Blob(["frame"], { type: "image/jpeg" })),
          "image/jpeg",
          JPEG_QUALITY,
        );
      } catch {
        resolve(new Blob(["frame"], { type: "image/jpeg" }));
      }
    });
  }, []);

  // Bounded preview loop: one in-flight request, fixed cadence.
  useEffect(() => {
    if (cameraState !== "live" || disabled) return;
    if (activeSlotIndex >= POSE_SLOTS.length) return;

    const tick = async () => {
      if (inFlightRef.current) return;
      // Skip when tab is backgrounded (document.hidden can be undefined in tests).
      if (typeof document !== "undefined" && document.hidden === true) return;
      inFlightRef.current = true;
      const ac = new AbortController();
      abortRef.current = ac;
      try {
        const blob = await grabJpegBlob();
        if (!blob || ac.signal.aborted) return;
        const result = await analyzeEnrollmentFrame(blob, "preview.jpg", ac.signal);
        if (ac.signal.aborted) return;
        setPreview(result);

        const slot = POSE_SLOTS[activeSlotIndex];
        if (!slot) return;

        if (!result.accepted) {
          setStatusText(reasonGuidance(result.reason));
          return;
        }

        if (!yawMatchesSlot(result.yaw, slot)) {
          setStatusText(slot.prompt);
          return;
        }

        // Already have this slot?
        if (capturedRef.current.some((c) => c.slot === slot.id)) {
          setStatusText("Pose acquired · next");
          return;
        }

        const now = Date.now();
        if (now - lastCaptureRef.current < CAPTURE_COOLDOWN_MS) {
          setStatusText("Hold… locking pose");
          return;
        }

        // Capture: re-encode current frame as File
        const file = new File([blob], `pose-${slot.id}.jpg`, {
          type: "image/jpeg",
        });
        const url = URL.createObjectURL(file);
        lastCaptureRef.current = now;
        const next = [
          ...capturedRef.current.filter((c) => c.slot !== slot.id),
          { slot: slot.id, file, url },
        ];
        // Sort by POSE_SLOTS order
        next.sort(
          (a, b) =>
            POSE_SLOTS.findIndex((s) => s.id === a.slot) -
            POSE_SLOTS.findIndex((s) => s.id === b.slot),
        );
        setCapturedAndEmit(next);
        setStatusText(`${slot.label} captured`);
      } catch (err) {
        if (ac.signal.aborted) return;
        // Soft-fail preview; keep camera running
        const msg = err instanceof Error ? err.message : "Preview failed";
        setStatusText(msg);
      } finally {
        if (abortRef.current === ac) {
          abortRef.current = null;
          inFlightRef.current = false;
        }
      }
    };

    const id = window.setInterval(tick, PREVIEW_INTERVAL_MS);
    // Kick immediately
    void tick();
    return () => {
      window.clearInterval(id);
      abortRef.current?.abort();
      abortRef.current = null;
      inFlightRef.current = false;
    };
  }, [
    cameraState,
    disabled,
    activeSlotIndex,
    grabJpegBlob,
    setCapturedAndEmit,
  ]);

  function removeSlot(slot: PoseSlotId) {
    const prev = capturedRef.current.find((c) => c.slot === slot);
    if (prev) URL.revokeObjectURL(prev.url);
    const next = capturedRef.current.filter((c) => c.slot !== slot);
    setCapturedAndEmit(next);
    setStatusText(`Retake ${slot.replace("_", " ")}`);
  }

  const complete = activeSlotIndex >= POSE_SLOTS.length && captured.length === POSE_SLOTS.length;
  const bbox = preview?.bbox;
  const filled = new Set(captured.map((c) => c.slot));

  return (
    <div
      className="space-y-3"
      data-testid="guided-face-capture"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs font-bold uppercase tracking-label text-body">
          Guided capture · {captured.length}/{POSE_SLOTS.length} poses
        </p>
        <div className="flex flex-wrap gap-2">
          {cameraState !== "live" ? (
            <button
              type="button"
              onClick={() => void startCamera()}
              disabled={disabled || complete || cameraState === "starting"}
              className="min-h-11 border border-ink px-4 py-2 text-xs font-bold uppercase tracking-label text-ink hover:bg-elevated focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan disabled:opacity-50"
              data-testid="start-camera"
            >
              {cameraState === "starting"
                ? "Starting…"
                : complete
                  ? "Capture complete"
                  : "Start camera"}
            </button>
          ) : (
            <button
              type="button"
              onClick={stopCamera}
              className="min-h-11 border border-hairline px-4 py-2 text-xs font-bold uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
              data-testid="stop-camera"
            >
              Stop camera
            </button>
          )}
        </div>
      </div>

      {(cameraState === "denied" || cameraState === "error") && (
        <div
          role="alert"
          className="border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger"
          data-testid="camera-permission-error"
        >
          {cameraError}
        </div>
      )}

      <div
        className="relative w-full overflow-hidden border border-hairline bg-soft"
        style={{ aspectRatio: videoAspect }}
        data-testid="capture-stage"
      >
        <video
          ref={videoRef}
          onLoadedMetadata={syncVideoAspect}
          onResize={syncVideoAspect}
          playsInline
          muted
          autoPlay
          className="absolute inset-0 h-full w-full object-cover"
          // Mirror selfie preview for natural operator feedback; analysis uses unmirrored canvas.
          style={{ transform: "scaleX(-1)" }}
          data-testid="capture-video"
        />
        <canvas ref={canvasRef} className="hidden" aria-hidden="true" />

        {/* Face target frame */}
        <div
          className="pointer-events-none absolute left-1/2 top-1/2 h-[55%] w-[42%] -translate-x-1/2 -translate-y-1/2 border border-ink/50"
          data-testid="face-target"
          aria-hidden="true"
        >
          <span className="absolute -left-px -top-px h-3 w-3 border-l border-t border-cyan" />
          <span className="absolute -right-px -top-px h-3 w-3 border-r border-t border-cyan" />
          <span className="absolute -bottom-px -left-px h-3 w-3 border-b border-l border-cyan" />
          <span className="absolute -bottom-px -right-px h-3 w-3 border-b border-r border-cyan" />
        </div>

        {/* Live server bbox (mirrored to match video) */}
        {bbox && cameraState === "live" && (
          <div
            className="pointer-events-none absolute border border-signal/80"
            data-testid="live-bbox"
            style={{
              left: `${(1 - bbox[2]) * 100}%`,
              top: `${bbox[1] * 100}%`,
              width: `${(bbox[2] - bbox[0]) * 100}%`,
              height: `${(bbox[3] - bbox[1]) * 100}%`,
            }}
          />
        )}

        {/* Scan line */}
        {cameraState === "live" && (
          <div
            className="capture-scan pointer-events-none absolute inset-x-0 h-px bg-cyan/60"
            aria-hidden="true"
          />
        )}

        {/* Overlay prompt */}
        <div className="pointer-events-none absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/80 to-transparent px-3 pb-3 pt-8">
          <p
            className="text-center font-mono text-xs text-ink"
            data-testid="capture-status"
            role="status"
            aria-live="polite"
          >
            {complete
              ? "All five poses acquired · ready to enroll"
              : statusText}
          </p>
          {preview?.yaw != null && (
            <p className="mt-1 text-center font-mono text-[10px] uppercase tracking-label text-muted">
              yaw {preview.yaw.toFixed(1)}° · faces {preview.face_count}
            </p>
          )}
        </div>
      </div>

      {/* Mission timeline slots */}
      <ol
        className="grid grid-cols-5 gap-1"
        data-testid="pose-slots"
        aria-label="Pose capture progress"
      >
        {POSE_SLOTS.map((slot, i) => {
          const done = filled.has(slot.id);
          const active = i === activeSlotIndex && !complete;
          return (
            <li key={slot.id}>
              <div
                className={`flex min-h-11 flex-col items-center justify-center border px-1 py-2 text-center ${
                  done
                    ? "border-signal/50 bg-signal/10 text-signal"
                    : active
                      ? "border-cyan text-ink"
                      : "border-hairline text-muted"
                }`}
                data-testid={`pose-slot-${slot.id}`}
                data-state={done ? "done" : active ? "active" : "pending"}
              >
                <span className="text-[10px] font-bold uppercase tracking-label">
                  {slot.label}
                </span>
                <span className="mt-0.5 font-mono text-[10px]">
                  {done ? "LOCK" : active ? "ACQ" : "—"}
                </span>
              </div>
            </li>
          );
        })}
      </ol>

      {/* Captured previews with remove/retake */}
      {captured.length > 0 && (
        <ul
          className="flex flex-wrap gap-3"
          data-testid="guided-previews"
          aria-label="Captured pose images"
        >
          {captured.map((c) => (
            <li key={c.slot} className="w-20 space-y-1">
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img
                src={c.url}
                alt={`${c.slot} capture`}
                className="h-16 w-16 border border-hairline object-cover"
              />
              <button
                type="button"
                onClick={() => removeSlot(c.slot)}
                disabled={disabled}
                className="min-h-11 w-full border border-hairline px-1 py-1 text-[10px] font-bold uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan disabled:opacity-50"
                data-testid={`retake-${c.slot}`}
              >
                Retake
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
