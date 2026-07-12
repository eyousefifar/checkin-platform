"use client";

import { useEffect, useRef, useState } from "react";
import { FaceHudCanvas } from "./FaceHudCanvas";
import { connectWhep, whepUrl, type WhepHandle } from "@/lib/whep";
import type { FaceDet } from "@/lib/types";

type VideoLifecycle = "connecting" | "playing" | "error";

const INITIAL_RETRY_MS = 2000;
const MAX_RETRY_MS = 10000;

export function CameraTile({
  cameraId,
  name,
  direction,
  online,
  faces,
  fps,
  webrtcPath,
}: {
  cameraId: string;
  name: string;
  direction: string;
  /** Capture status from camera_status WS messages; undefined = unknown. */
  online?: boolean;
  faces: FaceDet[];
  fps?: number;
  /** MediaMTX path from health; omitted while health is still retrying. */
  webrtcPath?: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const [size, setSize] = useState({ w: 640, h: 360 });
  const [videoState, setVideoState] = useState<VideoLifecycle>("connecting");
  const [videoError, setVideoError] = useState<string | null>(null);
  const webrtcBase =
    process.env.NEXT_PUBLIC_WEBRTC_BASE || "http://localhost:8889";

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setSize({ w: el.clientWidth, h: el.clientHeight });
    });
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  // Single WHEP playback lifecycle: connecting → playing | error (with retry).
  // SDP success is not "playing"; only the video `playing` event is.
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    // Do not assume a path while health has not resolved webrtcPath.
    if (!webrtcPath) {
      setVideoState("connecting");
      setVideoError(null);
      return;
    }

    let handle: WhepHandle | null = null;
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;
    let retryAttempt = 0;

    const endpoint = whepUrl(webrtcBase, webrtcPath);

    const clearRetry = () => {
      if (retryTimer != null) {
        clearTimeout(retryTimer);
        retryTimer = null;
      }
    };

    const closeHandle = () => {
      if (handle) {
        handle.close();
        handle = null;
      }
      if (video.srcObject) {
        video.srcObject = null;
      }
    };

    const scheduleRetry = (reason: string) => {
      if (cancelled) return;
      setVideoState("error");
      setVideoError(reason);
      clearRetry();
      const delay = Math.min(MAX_RETRY_MS, INITIAL_RETRY_MS * 2 ** retryAttempt);
      retryAttempt += 1;
      retryTimer = setTimeout(() => {
        if (!cancelled) void start();
      }, delay);
    };

    const onPlaying = () => {
      if (cancelled) return;
      retryAttempt = 0;
      setVideoState("playing");
      setVideoError(null);
    };

    const onWaitingOrStalled = () => {
      if (cancelled) return;
      // Leave playing and surface reconnect intent without treating as terminal.
      setVideoState((prev) => (prev === "playing" ? "connecting" : prev));
    };

    const onVideoError = () => {
      if (cancelled) return;
      scheduleRetry("Video element error — retrying WHEP");
    };

    const onTrackEnded = () => {
      if (cancelled) return;
      scheduleRetry("Media track ended — retrying WHEP");
    };

    video.addEventListener("playing", onPlaying);
    video.addEventListener("waiting", onWaitingOrStalled);
    video.addEventListener("stalled", onWaitingOrStalled);
    video.addEventListener("error", onVideoError);
    video.addEventListener("wheptrackended", onTrackEnded);

    const start = async () => {
      if (cancelled) return;
      clearRetry();
      closeHandle();
      setVideoState("connecting");
      setVideoError(null);
      try {
        handle = await connectWhep(endpoint, video);
        if (cancelled) {
          handle.close();
          handle = null;
          return;
        }

        // Connection failure after SDP success.
        handle.pc.onconnectionstatechange = () => {
          if (cancelled || !handle) return;
          const st = handle.pc.connectionState;
          if (st === "failed" || st === "disconnected" || st === "closed") {
            scheduleRetry(`WebRTC ${st} — retrying WHEP`);
          }
        };
        // Stay in connecting until `playing` fires — SDP alone is not live.
      } catch (err) {
        if (cancelled) return;
        const raw = err instanceof Error ? err.message : "WHEP failed";
        const display =
          raw.toLowerCase().includes("network") || raw.includes("fetch")
            ? `MediaMTX unreachable at ${webrtcBase} (docker compose up -d mediamtx?)`
            : raw;
        scheduleRetry(display);
      }
    };

    void start();

    return () => {
      cancelled = true;
      clearRetry();
      video.removeEventListener("playing", onPlaying);
      video.removeEventListener("waiting", onWaitingOrStalled);
      video.removeEventListener("stalled", onWaitingOrStalled);
      video.removeEventListener("error", onVideoError);
      video.removeEventListener("wheptrackended", onTrackEnded);
      closeHandle();
    };
  }, [webrtcBase, webrtcPath]);

  const showVideo = videoState === "playing";
  const cameraLabel =
    online === true ? "ONLINE" : online === false ? "OFFLINE" : "UNKNOWN";
  const cameraStatus =
    online === true ? "online" : online === false ? "offline" : "unknown";
  const videoLabel =
    videoState === "playing"
      ? "VIDEO LIVE"
      : videoState === "connecting"
        ? "CONNECTING"
        : "VIDEO ERROR";

  return (
    <div
      ref={containerRef}
      className="hud-brackets relative aspect-video w-full overflow-hidden border border-hairline bg-soft scanline"
      data-camera={cameraId}
      data-webrtc-path={webrtcPath || ""}
      data-video-state={videoState}
    >
      <span className="bracket-bl" />
      <span className="bracket-br" />

      <video
        ref={videoRef}
        className={`absolute inset-0 z-0 h-full w-full object-cover ${
          showVideo ? "opacity-100" : "opacity-0"
        }`}
        playsInline
        muted
        autoPlay
        data-testid="camera-video"
      />

      {!showVideo && (
        <div className="absolute inset-0 z-[1] flex items-center justify-center bg-gradient-to-b from-[#0a0a0a] to-[#111]">
          <div className="px-4 text-center">
            <div
              className="font-mono text-xs uppercase tracking-label text-body"
              role="status"
              aria-live="polite"
            >
              {!webrtcPath
                ? "Health retrying · waiting for stream path"
                : online === true
                  ? "Vision online · awaiting WebRTC"
                  : online === false
                    ? "Camera offline"
                    : "Camera status unknown"}
            </div>
            <div className="mt-1 text-xs text-body">
              {webrtcPath ? `${webrtcBase}/${webrtcPath}` : "path pending"}
            </div>
            <div
              className="mt-2 font-mono text-xs text-body"
              data-testid="video-status-text"
            >
              WHEP · {videoError || (videoState === "connecting" ? "connecting…" : videoState)}
            </div>
            <div className="mt-3 text-xs text-body">
              Canvas HUD from WS · video via MediaMTX when stream is live
            </div>
          </div>
        </div>
      )}

      <FaceHudCanvas faces={faces} width={size.w} height={size.h} />

      <div className="absolute left-3 top-3 z-30 flex flex-wrap items-center gap-2">
        <span className="bg-black/70 px-2 py-1 text-xs font-bold uppercase tracking-label text-ink">
          {name}
        </span>
        <span className="border border-m-blue-dark/60 bg-black/70 px-2 py-1 text-xs font-bold uppercase tracking-label text-ink">
          {direction}
        </span>
      </div>
      <div className="absolute right-3 top-3 z-30 flex flex-wrap items-center justify-end gap-2">
        <span
          className={`px-2 py-1 text-xs font-bold uppercase tracking-label ${
            online === true
              ? "bg-success/20 text-success"
              : online === false
                ? "bg-m-red/20 text-m-red"
                : "bg-black/70 text-body"
          }`}
          data-camera-status={cameraStatus}
          data-testid="camera-capture-badge"
        >
          {cameraLabel}
        </span>
        <span
          className={`px-2 py-1 text-xs font-bold uppercase tracking-label ${
            videoState === "playing"
              ? "bg-success/20 text-success"
              : videoState === "error"
                ? "bg-m-red/20 text-m-red"
                : "bg-black/70 text-body"
          }`}
          data-testid="browser-video-badge"
          data-video-badge={videoState}
        >
          {videoLabel}
        </span>
        {fps != null && (
          <span className="bg-black/70 px-2 py-1 font-mono text-xs text-body">
            {fps.toFixed(1)} FPS
          </span>
        )}
      </div>
    </div>
  );
}
