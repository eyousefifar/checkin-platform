"use client";

import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { connectWhep, whepUrl, type WhepHandle } from "@/lib/whep";
import { useHealth } from "./useHealth";

export type CameraSession = {
  path: string;
  stream: MediaStream | null;
  state: "connecting" | "connected" | "error";
  error: string | null;
};

const CameraSessionsContext = createContext<Record<string, CameraSession> | null>(null);

function CameraSessionWorker({
  cameraId,
  path,
  setSessions,
}: {
  cameraId: string;
  path: string;
  setSessions: React.Dispatch<React.SetStateAction<Record<string, CameraSession>>>;
}) {
  useEffect(() => {
    let cancelled = false;
    let handle: WhepHandle | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;
    let disconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let attempt = 0;
    const base = process.env.NEXT_PUBLIC_WEBRTC_BASE || "http://localhost:8889";
    const endpoint = whepUrl(base, path);

    const update = (next: Omit<CameraSession, "path">) => {
      if (cancelled) return;
      setSessions((prev) => ({ ...prev, [cameraId]: { path, ...next } }));
    };

    const clearTimers = () => {
      if (retryTimer) clearTimeout(retryTimer);
      if (disconnectTimer) clearTimeout(disconnectTimer);
      retryTimer = null;
      disconnectTimer = null;
    };

    const closeCurrent = () => {
      if (!handle) return;
      handle.pc.onconnectionstatechange = null;
      handle.close();
      handle = null;
    };

    const scheduleRetry = (reason: string) => {
      if (cancelled || retryTimer) return;
      if (disconnectTimer) clearTimeout(disconnectTimer);
      disconnectTimer = null;
      closeCurrent();
      update({ stream: null, state: "error", error: reason });
      const delay = Math.min(5000, 500 * 2 ** attempt++);
      retryTimer = setTimeout(() => {
        retryTimer = null;
        void start();
      }, delay);
    };

    const watchTrack = (track: MediaStreamTrack) => {
      track.addEventListener(
        "ended",
        () => scheduleRetry("Media track ended — retrying WHEP"),
        { once: true },
      );
    };

    const start = async () => {
      if (cancelled) return;
      update({ stream: null, state: "connecting", error: null });
      try {
        const next = await connectWhep(endpoint);
        if (cancelled) {
          next.close();
          return;
        }
        handle = next;
        next.stream.getTracks().forEach(watchTrack);
        next.stream.addEventListener("addtrack", (event) => watchTrack(event.track));
        next.pc.onconnectionstatechange = () => {
          if (cancelled || handle !== next) return;
          const state = next.pc.connectionState;
          if (state === "connected") {
            if (disconnectTimer) clearTimeout(disconnectTimer);
            disconnectTimer = null;
            attempt = 0;
            update({ stream: next.stream, state: "connected", error: null });
          } else if (state === "disconnected") {
            update({ stream: next.stream, state: "connecting", error: null });
            if (!disconnectTimer) {
              disconnectTimer = setTimeout(() => {
                disconnectTimer = null;
                if (next.pc.connectionState === "disconnected") {
                  scheduleRetry("WebRTC disconnected — retrying WHEP");
                }
              }, 1000);
            }
          } else if (state === "failed" || state === "closed") {
            scheduleRetry(`WebRTC ${state} — retrying WHEP`);
          }
        };
        update({ stream: next.stream, state: "connected", error: null });
      } catch (error) {
        const message = error instanceof Error ? error.message : "WHEP failed";
        scheduleRetry(message);
      }
    };

    void start();
    return () => {
      cancelled = true;
      clearTimers();
      closeCurrent();
      setSessions((prev) => {
        if (prev[cameraId]?.path !== path) return prev;
        const next = { ...prev };
        delete next[cameraId];
        return next;
      });
    };
  }, [cameraId, path, setSessions]);

  return null;
}

export function CameraSessionsProvider({ children }: { children: ReactNode }) {
  const { data: health } = useHealth();
  const [sessions, setSessions] = useState<Record<string, CameraSession>>({});
  const cameras = (health?.cameras ?? []).filter(
    (camera) => camera.enabled && camera.webrtc_path,
  );

  return (
    <CameraSessionsContext.Provider value={sessions}>
      {cameras.map((camera) => (
        <CameraSessionWorker
          key={`${camera.id}:${camera.webrtc_path}`}
          cameraId={camera.id}
          path={camera.webrtc_path}
          setSessions={setSessions}
        />
      ))}
      {children}
    </CameraSessionsContext.Provider>
  );
}

export function useCameraSession(cameraId: string): CameraSession | undefined {
  const sessions = useContext(CameraSessionsContext);
  if (!sessions) {
    throw new Error("useCameraSession must be used inside CameraSessionsProvider");
  }
  return sessions[cameraId];
}
