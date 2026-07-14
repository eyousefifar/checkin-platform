"use client";

import { useEffect, useRef, useState } from "react";
import { FaceHudCanvas } from "./FaceHudCanvas";
import { useCameraSession } from "@/hooks/useCameraSessions";
import type { FaceDet } from "@/lib/types";

type VideoLifecycle = "connecting" | "playing" | "error";

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
  const session = useCameraSession(cameraId);
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

  // Attach the provider-owned stream. This component never owns or stops tracks.
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    if (!webrtcPath || !session?.stream) {
      video.srcObject = null;
      setVideoState("connecting");
      setVideoError(null);
      return;
    }

    const onPlaying = () => {
      setVideoState("playing");
      setVideoError(null);
    };

    const onWaitingOrStalled = () => {
      setVideoState((prev) => (prev === "playing" ? "connecting" : prev));
    };

    const onVideoError = () => {
      setVideoState("error");
      setVideoError("Video playback error");
    };

    video.addEventListener("playing", onPlaying);
    video.addEventListener("waiting", onWaitingOrStalled);
    video.addEventListener("stalled", onWaitingOrStalled);
    video.addEventListener("error", onVideoError);
    video.srcObject = session.stream;
    setVideoState("connecting");
    setVideoError(null);
    void video.play().catch(() => {
      /* autoplay policy: remain connecting until the playing event */
    });

    return () => {
      video.removeEventListener("playing", onPlaying);
      video.removeEventListener("waiting", onWaitingOrStalled);
      video.removeEventListener("stalled", onWaitingOrStalled);
      video.removeEventListener("error", onVideoError);
      video.srcObject = null;
    };
  }, [session?.stream, webrtcPath]);

  useEffect(() => {
    if (!webrtcPath || !session || session.state === "connecting") {
      setVideoState((previous) =>
        previous === "playing" ? "connecting" : previous,
      );
      return;
    }
    if (session.state === "error") {
      setVideoState("error");
      setVideoError(session.error);
    }
  }, [session, webrtcPath]);

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
