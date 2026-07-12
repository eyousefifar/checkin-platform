"use client";

import { CameraTile } from "@/components/CameraTile";
import { EventTicker } from "@/components/EventTicker";
import { MetricPill } from "@/components/MetricPill";
import { useLiveWs } from "@/hooks/useLiveWs";

export default function DashboardPage() {
  const { connected, detections, events, metrics, cameraOnline } = useLiveWs();
  const camInOnline = cameraOnline["cam_in"] ?? connected;
  const fps = metrics?.vision_fps?.cam_in;

  return (
    <div className="dashboard-grid min-h-[calc(100vh-7rem)] p-6">
      <div className="mb-4 flex items-end justify-between">
        <div>
          <h1 className="text-2xl font-bold uppercase tracking-wide text-ink">
            Live operations
          </h1>
          <p className="mt-1 text-sm text-body">
            On-prem vision · WebSocket HUD · no cloud face APIs
          </p>
        </div>
        <div
          className={`text-[11px] font-bold uppercase tracking-label ${
            connected ? "text-success" : "text-warning"
          }`}
        >
          {connected ? "WS linked" : "WS reconnecting…"}
        </div>
      </div>

      <div className="mb-6 grid grid-cols-2 gap-3 md:grid-cols-4">
        <MetricPill label="Cameras" value={metrics?.cameras_online ?? (camInOnline ? 1 : 0)} />
        <MetricPill label="Present" value={metrics?.present_count ?? "—"} />
        <MetricPill label="Events today" value={metrics?.events_today ?? events.length} />
        <MetricPill label="Vision FPS" value={fps != null ? fps.toFixed(1) : "—"} />
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <div className="space-y-4 lg:col-span-2">
          <CameraTile
            cameraId="cam_in"
            name="Entrance"
            direction="IN / BI"
            online={camInOnline}
            faces={detections["cam_in"] || []}
            fps={fps}
            webrtcPath="demo"
          />
        </div>
        <div className="min-h-[320px] lg:min-h-0">
          <EventTicker events={events} />
        </div>
      </div>
    </div>
  );
}
