"use client";

import { useMemo } from "react";
import { CameraTile } from "@/components/CameraTile";
import { EventTicker } from "@/components/EventTicker";
import { MetricPill } from "@/components/MetricPill";
import { useHealth } from "@/hooks/useHealth";
import { useLiveWs } from "@/hooks/useLiveWs";
import type { HealthCamera } from "@/lib/types";

/** Appliance UI supports at most two simultaneous WHEP streams. */
const MAX_WALL_CAMERAS = 2;

export default function DashboardPage() {
  const { connected, detections, events, metrics, cameraOnline } = useLiveWs();
  const { data: health, loading: healthLoading, error: healthError } = useHealth();

  // Preserve health list order; never hardcode cam_in.
  const enabledAll = useMemo(
    () => (health?.cameras ?? []).filter((c: HealthCamera) => c.enabled),
    [health],
  );
  const wallCameras = enabledAll.slice(0, MAX_WALL_CAMERAS);
  const excessEnabled = enabledAll.length > MAX_WALL_CAMERAS;

  // Health has not returned yet — show retry state, not offline cameras.
  const healthPending = health == null;
  const healthRetrying = healthPending && (healthLoading || Boolean(healthError));

  const primaryFps =
    wallCameras.length > 0
      ? metrics?.vision_fps?.[wallCameras[0].id]
      : undefined;

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
          {(healthRetrying || healthError) && (
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
          value={
            metrics?.cameras_online ??
            wallCameras.filter((c) => cameraOnline[c.id] === true).length
          }
        />
        <MetricPill label="Present" value={metrics?.present_count ?? "—"} />
        <MetricPill
          label="Events today"
          value={metrics?.events_today ?? "—"}
        />
        <MetricPill
          label="Vision FPS"
          value={primaryFps != null ? primaryFps.toFixed(1) : "—"}
        />
      </div>

      {excessEnabled && (
        <div
          className="mb-4 border border-warning/40 bg-warning/10 px-4 py-2 text-xs text-warning"
          data-testid="camera-cap-warning"
          role="status"
        >
          This appliance UI supports two cameras. Showing the first two enabled
          cameras ({enabledAll.length} enabled in health).
        </div>
      )}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <div className="min-w-0 space-y-4 lg:col-span-2">
          {healthPending ? (
            <div
              className="flex aspect-video w-full items-center justify-center border border-hairline bg-card"
              data-testid="health-camera-state"
            >
              <div className="px-6 text-center">
                <div className="text-sm font-bold uppercase tracking-label text-warning">
                  Health retrying
                </div>
                <p className="mt-2 text-xs text-body">
                  Waiting for public camera list before starting video streams.
                  {healthError ? ` (${healthError})` : ""}
                </p>
              </div>
            </div>
          ) : wallCameras.length === 0 ? (
            <div
              className="flex aspect-video w-full items-center justify-center border border-hairline bg-card"
              data-testid="no-enabled-cameras"
            >
              <div className="max-w-md px-6 text-center">
                <div className="text-sm font-bold uppercase tracking-label text-ink">
                  No enabled cameras
                </div>
                <p className="mt-2 text-xs text-body">
                  Enable one or two RTSP cameras in the edge deployment
                  configuration (CAM_IN_RTSP / CAM_OUT_RTSP and related env).
                  See deployment docs — camera setup is operator-side for this
                  MVP.
                </p>
              </div>
            </div>
          ) : (
            <div
              className={
                wallCameras.length === 1
                  ? "space-y-4"
                  : "grid grid-cols-1 gap-4 md:grid-cols-2"
              }
              data-testid="camera-wall"
              data-camera-count={wallCameras.length}
            >
              {wallCameras.map((cam) => (
                <CameraTile
                  key={cam.id}
                  cameraId={cam.id}
                  name={cam.name}
                  direction={cam.direction}
                  online={cameraOnline[cam.id]}
                  faces={detections[cam.id] || []}
                  fps={metrics?.vision_fps?.[cam.id]}
                  webrtcPath={cam.webrtc_path}
                />
              ))}
            </div>
          )}
        </div>
        <div className="min-h-[320px] min-w-0 lg:min-h-0">
          <EventTicker events={events} />
        </div>
      </div>
    </div>
  );
}
