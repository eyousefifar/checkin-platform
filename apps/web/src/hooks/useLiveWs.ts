"use client";

import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { wsUrl } from "@/lib/api";
import type { AttendanceMsg, DetectionsMsg, FaceDet, MetricsMsg } from "@/lib/types";

/** HUD face sets older than this without a refresh are cleared. */
export const DETECTION_FRESHNESS_MS = 500;

export type CameraOnlineMap = Record<string, boolean | undefined>;

export type LiveWsState = ReturnType<typeof useLiveWsState>;
const LiveWsContext = createContext<LiveWsState | null>(null);

function useLiveWsState() {
  const [connected, setConnected] = useState(false);
  const [detections, setDetections] = useState<Record<string, FaceDet[]>>({});
  const [events, setEvents] = useState<AttendanceMsg[]>([]);
  const [metrics, setMetrics] = useState<MetricsMsg | null>(null);
  const [cameraOnline, setCameraOnline] = useState<CameraOnlineMap>({});
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const stoppedRef = useRef(false);
  const detectionAtRef = useRef<Record<string, number>>({});

  const clearAllDetections = useCallback(() => {
    detectionAtRef.current = {};
    setDetections({});
  }, []);

  const connect = useCallback(() => {
    if (typeof window === "undefined" || stoppedRef.current) return;
    try {
      if (wsRef.current) {
        const prev = wsRef.current;
        wsRef.current = null;
        try {
          prev.onclose = null;
          prev.onerror = null;
          prev.onmessage = null;
          prev.close();
        } catch {
          /* ignore */
        }
      }
      const ws = new WebSocket(wsUrl());
      wsRef.current = ws;
      ws.onopen = () => {
        if (stoppedRef.current) return;
        setConnected(true);
        retryRef.current = 0;
      };
      ws.onclose = () => {
        setConnected(false);
        clearAllDetections();
        if (stoppedRef.current) return;
        const delay = Math.min(10000, 500 * 2 ** retryRef.current);
        retryRef.current += 1;
        if (timerRef.current) clearTimeout(timerRef.current);
        timerRef.current = setTimeout(connect, delay);
      };
      ws.onerror = () => {
        try {
          ws.close();
        } catch {
          /* ignore */
        }
      };
      ws.onmessage = (ev) => {
        try {
          const msg = JSON.parse(ev.data as string);
          if (msg.type === "detections") {
            const d = msg as DetectionsMsg;
            const now =
              typeof performance !== "undefined" && performance.now
                ? performance.now()
                : Date.now();
            detectionAtRef.current[d.camera_id] = now;
            setDetections((prev) => ({ ...prev, [d.camera_id]: d.faces }));
          } else if (msg.type === "attendance") {
            setEvents((prev) => [msg as AttendanceMsg, ...prev].slice(0, 40));
          } else if (msg.type === "metrics") {
            setMetrics(msg as MetricsMsg);
          } else if (msg.type === "camera_status") {
            const camId = msg.camera_id as string;
            const online = !!msg.online;
            setCameraOnline((prev) => ({
              ...prev,
              [camId]: online,
            }));
            if (!online) {
              delete detectionAtRef.current[camId];
              setDetections((prev) => {
                if (!(camId in prev)) return prev;
                const next = { ...prev };
                delete next[camId];
                return next;
              });
            }
          }
        } catch {
          /* ignore malformed */
        }
      };
    } catch {
      if (stoppedRef.current) return;
      timerRef.current = setTimeout(connect, 2000);
    }
  }, [clearAllDetections]);

  useEffect(() => {
    stoppedRef.current = false;
    connect();
    const expiry = setInterval(() => {
      const now =
        typeof performance !== "undefined" && performance.now
          ? performance.now()
          : Date.now();
      const stale: string[] = [];
      for (const [cam, at] of Object.entries(detectionAtRef.current)) {
        if (now - at >= DETECTION_FRESHNESS_MS) {
          stale.push(cam);
        }
      }
      if (stale.length === 0) return;
      for (const cam of stale) {
        delete detectionAtRef.current[cam];
      }
      setDetections((prev) => {
        let changed = false;
        const next = { ...prev };
        for (const cam of stale) {
          if (cam in next) {
            delete next[cam];
            changed = true;
          }
        }
        return changed ? next : prev;
      });
    }, 100);

    return () => {
      stoppedRef.current = true;
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      clearInterval(expiry);
      const ws = wsRef.current;
      wsRef.current = null;
      if (ws) {
        try {
          ws.onclose = null;
          ws.onerror = null;
          ws.onmessage = null;
          ws.close();
        } catch {
          /* ignore */
        }
      }
    };
  }, [connect]);

  return { connected, detections, events, metrics, cameraOnline };
}

export function LiveWsProvider({ children }: { children: ReactNode }) {
  const value = useLiveWsState();
  return createElement(LiveWsContext.Provider, { value }, children);
}

export function useLiveWs(): LiveWsState {
  const value = useContext(LiveWsContext);
  if (!value) throw new Error("useLiveWs must be used inside LiveWsProvider");
  return value;
}
