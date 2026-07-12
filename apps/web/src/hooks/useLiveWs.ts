"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { wsUrl } from "@/lib/api";
import type { AttendanceMsg, DetectionsMsg, FaceDet, MetricsMsg } from "@/lib/types";

export function useLiveWs() {
  const [connected, setConnected] = useState(false);
  const [detections, setDetections] = useState<Record<string, FaceDet[]>>({});
  const [events, setEvents] = useState<AttendanceMsg[]>([]);
  const [metrics, setMetrics] = useState<MetricsMsg | null>(null);
  const [cameraOnline, setCameraOnline] = useState<Record<string, boolean>>({});
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const connect = useCallback(() => {
    if (typeof window === "undefined") return;
    try {
      const ws = new WebSocket(wsUrl());
      wsRef.current = ws;
      ws.onopen = () => {
        setConnected(true);
        retryRef.current = 0;
      };
      ws.onclose = () => {
        setConnected(false);
        const delay = Math.min(10000, 500 * 2 ** retryRef.current);
        retryRef.current += 1;
        timerRef.current = setTimeout(connect, delay);
      };
      ws.onerror = () => ws.close();
      ws.onmessage = (ev) => {
        try {
          const msg = JSON.parse(ev.data);
          if (msg.type === "detections") {
            const d = msg as DetectionsMsg;
            setDetections((prev) => ({ ...prev, [d.camera_id]: d.faces }));
          } else if (msg.type === "attendance") {
            setEvents((prev) => [msg as AttendanceMsg, ...prev].slice(0, 40));
          } else if (msg.type === "metrics") {
            setMetrics(msg as MetricsMsg);
          } else if (msg.type === "camera_status") {
            setCameraOnline((prev) => ({
              ...prev,
              [msg.camera_id]: !!msg.online,
            }));
          }
        } catch {
          /* ignore */
        }
      };
    } catch {
      timerRef.current = setTimeout(connect, 2000);
    }
  }, []);

  useEffect(() => {
    connect();
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      wsRef.current?.close();
    };
  }, [connect]);

  return { connected, detections, events, metrics, cameraOnline };
}
