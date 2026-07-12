"use client";

import { CameraTile } from "@/components/CameraTile";
import { EventTicker } from "@/components/EventTicker";
import { MetricPill } from "@/components/MetricPill";
import { useHealth } from "@/hooks/useHealth";
import { useLiveWs } from "@/hooks/useLiveWs";

export default function DashboardPage() {
  const { connected, detections, events, metrics, cameraOnline } = useLiveWs();
  const { data: health, loading: healthLoading, error: healthError } = useHealth();
  // Do not infer camera capture from WS transport connectivity.
  const camInOnline = cameraOnline["cam_in"];
  const fps = metrics?.vision_fps?.cam_in;

  const camIn = health?.cameras?.find((c) => c.id === "cam_in");
  // Only pass a path once health has returned one — never hammer a guessed path.
  const webrtcPath = camIn?.webrtc_path;

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
        <div className="flex items-center gap-3">
          {(healthLoading || healthError || !health) && (
            <div
              className="text-[11px] font-bold uppercase tracking-label text-warning"
              data-testid="health-retrying"
            >
              {healthError ? `Health retrying · ${healthError}` : "Health retrying…"}
            </div>
          )}
          <div
            className={`text-[11px] font-bold uppercase tracking-label ${
              connected ? "text-success" : "text-warning"
            }`}
          >
            {connected ? "WS linked" : "WS reconnecting…"}
          </div>
        </div>
      </div>

      <div className="mb-6 grid grid-cols-2 gap-3 md:grid-cols-4">
        <MetricPill
          label="Cameras"
          value={metrics?.cameras_online ?? (camInOnline === true ? 1 : 0)}
        />
        <MetricPill label="Present" value={metrics?.present_count ?? "—"} />
        <MetricPill
          label="Events today"
          value={metrics?.events_today ?? "—"}
        />
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
            webrtcPath={webrtcPath}
          />
        </div>
        <div className="min-h-[320px] lg:min-h-0">
          <EventTicker events={events} />
        </div>
      </div>
    </div>
  );
}
