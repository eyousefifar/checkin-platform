"use client";

import { useEffect, useRef, useState } from "react";
import { FaceHudCanvas } from "./FaceHudCanvas";
import { connectWhep, whepUrl, type WhepHandle } from "@/lib/whep";
import type { FaceDet } from "@/lib/types";

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
  online: boolean;
  faces: FaceDet[];
  fps?: number;
  webrtcPath?: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const [size, setSize] = useState({ w: 640, h: 360 });
  const [videoOk, setVideoOk] = useState(false);
  const [videoError, setVideoError] = useState<string | null>(null);
  const [useHlsFallback, setUseHlsFallback] = useState(false);
  const webrtcBase =
    process.env.NEXT_PUBLIC_WEBRTC_BASE || "http://localhost:8889";
  const path = webrtcPath || (cameraId === "cam_in" ? "demo" : cameraId);

  // HLS fallback URL (MediaMTX serves HLS on 8888 from the same path)
  const hlsBase = webrtcBase.replace(":8889", ":8888");
  const hlsUrl = `${hlsBase}/${path}/index.m3u8`;

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

  // WHEP live video path (MediaMTX) with HLS fallback for codec issues (e.g. H265 source)
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    let handle: WhepHandle | null = null;
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    const endpoint = whepUrl(webrtcBase, path);

    const tryHls = () => {
      if (cancelled) return;
      setUseHlsFallback(true);
      setVideoOk(false);
      setVideoError("WHEP codec incompatible (H265 source) — using HLS fallback");
      video.srcObject = null;
      video.src = hlsUrl;
      video.play().catch(() => {});
      // HLS is one-shot; no auto WHEP retry while in fallback
    };

    const start = async () => {
      if (useHlsFallback) return; // stay on HLS once chosen
      setVideoError(null);
      setUseHlsFallback(false);
      try {
        handle = await connectWhep(endpoint, video);
        if (cancelled) {
          handle.close();
          return;
        }
        setVideoOk(true);
      } catch (err) {
        if (cancelled) return;
        setVideoOk(false);
        const raw = err instanceof Error ? err.message : "WHEP failed";
        if (raw.includes("400")) {
          // Codec problem (very common with H265/HEVC cameras + standard browser WebRTC)
          setVideoError("WHEP 400 (codec mismatch — H265 source vs browser)");
          tryHls();
          return;
        }
        const display =
          raw.toLowerCase().includes("network") || raw.includes("fetch")
            ? `MediaMTX unreachable at ${webrtcBase} (docker compose up -d mediamtx?)`
            : raw;
        setVideoError(display);
        // retry WHEP
        retryTimer = setTimeout(start, 4000);
      }
    };

    void start();

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
      handle?.close();
      // cleanup HLS if active
      if (video.src) {
        video.pause();
        video.src = "";
      }
    };
  }, [webrtcBase, path, hlsUrl, useHlsFallback]);

  const showVideo = videoOk || useHlsFallback;

  return (
    <div
      ref={containerRef}
      className="hud-brackets relative aspect-video w-full overflow-hidden border border-hairline bg-soft scanline"
      data-camera={cameraId}
      data-webrtc-path={path}
    >
      <span className="bracket-bl" />
      <span className="bracket-br" />

      <video
        ref={videoRef}
        className={`absolute inset-0 z-0 h-full w-full object-cover ${
          showVideo || useHlsFallback ? "opacity-100" : "opacity-0"
        }`}
        playsInline
        muted
        autoPlay
        // For HLS fallback we set .src ; for WHEP we set .srcObject in connectWhep
        src={useHlsFallback ? hlsUrl : undefined}
        data-testid="camera-video"
      />

      {!showVideo && !useHlsFallback && (
        <div className="absolute inset-0 z-[1] flex items-center justify-center bg-gradient-to-b from-[#0a0a0a] to-[#111]">
          <div className="text-center px-4">
            <div className="font-mono text-[10px] uppercase tracking-label text-muted">
              {online ? "Vision online · awaiting WebRTC" : "Camera offline"}
            </div>
            <div className="mt-1 text-xs text-body">
              {webrtcBase}/{path}
            </div>
            <div className="mt-2 font-mono text-[10px] text-muted">
              WHEP · {videoError || "connecting…"}
            </div>
            <div className="mt-3 text-[10px] text-muted">
              Canvas HUD from WS · video via MediaMTX when stream is live
            </div>
          </div>
        </div>
      )}

      {/* Small indicator when HLS fallback is active */}
      {useHlsFallback && (
        <div className="absolute top-2 right-2 z-30 bg-black/60 px-1.5 py-0.5 text-[9px] text-muted">
          HLS fallback
        </div>
      )}

      <FaceHudCanvas faces={faces} width={size.w} height={size.h} />

      <div className="absolute left-3 top-3 z-30 flex items-center gap-2">
        <span className="bg-black/70 px-2 py-1 text-[10px] font-bold uppercase tracking-label text-ink">
          {name}
        </span>
        <span className="bg-black/70 px-2 py-1 text-[10px] uppercase tracking-label text-m-blue-light">
          {direction}
        </span>
      </div>
      <div className="absolute right-3 top-3 z-30 flex items-center gap-2">
        <span
          className={`px-2 py-1 text-[10px] font-bold uppercase tracking-label ${
            online || videoOk
              ? "bg-success/20 text-success"
              : "bg-m-red/20 text-m-red"
          }`}
        >
          {online || videoOk ? "ONLINE" : "OFFLINE"}
        </span>
        {fps != null && (
          <span className="bg-black/70 px-2 py-1 font-mono text-[10px] text-muted">
            {fps.toFixed(1)} FPS
          </span>
        )}
      </div>
    </div>
  );
}
