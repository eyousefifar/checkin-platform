"use client";

import { useMemo } from "react";
import { CameraTile } from "@/components/CameraTile";
import { EventTicker } from "@/components/EventTicker";
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

  const camerasOnline =
    metrics?.cameras_online ??
    wallCameras.filter((c) => cameraOnline[c.id] === true).length;

  return (
    <div className="dashboard-grid flex min-h-[calc(100vh-4.5rem)] min-w-0 flex-col">
      {/* Sparse mission header — not a marketing hero */}
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-3 border-b border-hairline px-4 py-3 md:px-6">
        <div className="min-w-0">
          <p className="text-[10px] font-bold uppercase tracking-label text-muted">
            Monitor
          </p>
          <h1 className="text-lg font-bold uppercase tracking-wide text-ink md:text-xl">
            Live operations
          </h1>
        </div>
        <div className="flex flex-wrap items-center gap-4">
          {(healthRetrying || healthError) && (
            <div
              className="text-[10px] font-bold uppercase tracking-label text-warning"
              data-testid="health-retrying"
              role="status"
              aria-live="polite"
            >
              {healthError ? `Health retrying · ${healthError}` : "Health retrying…"}
            </div>
          )}
          <div
            className={`text-[10px] font-bold uppercase tracking-label ${
              connected ? "text-signal" : "text-warning"
            }`}
            role="status"
            aria-live="polite"
            data-testid="ws-connection-status"
          >
            {connected ? "WS linked" : "WS reconnecting…"}
          </div>
        </div>
      </div>

      {/* Telemetry rail — sparse, not equal feature tiles */}
      <div
        className="flex min-w-0 flex-wrap items-stretch gap-0 border-b border-hairline font-mono text-xs"
        data-testid="telemetry-rail"
        role="group"
        aria-label="Live telemetry"
      >
        <TelemetryCell
          label="Cameras"
          value={String(camerasOnline)}
          testId="metric-Cameras"
        />
        <TelemetryCell
          label="Present"
          value={metrics?.present_count != null ? String(metrics.present_count) : "—"}
          testId="metric-Present"
        />
        <TelemetryCell
          label="Events today"
          value={metrics?.events_today != null ? String(metrics.events_today) : "—"}
          testId="metric-Events today"
        />
        <TelemetryCell
          label="Vision FPS"
          value={primaryFps != null ? primaryFps.toFixed(1) : "—"}
          testId="metric-Vision FPS"
          last
        />
      </div>

      {excessEnabled && (
        <div
          className="border-b border-warning/40 bg-warning/10 px-4 py-2 text-xs text-warning"
          data-testid="camera-cap-warning"
          role="status"
        >
          This appliance UI supports two cameras. Showing the first two enabled
          cameras ({enabledAll.length} enabled in health).
        </div>
      )}

      {/* Dominant camera wall + mission log */}
      <div className="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-12">
        <div className="min-w-0 space-y-0 border-hairline lg:col-span-8 lg:border-r">
          {healthPending ? (
            <div
              className="flex aspect-video w-full items-center justify-center bg-card"
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
              className="flex aspect-video w-full items-center justify-center bg-card"
              data-testid="no-enabled-cameras"
            >
              <div className="px-6 text-center">
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
                  ? "space-y-0"
                  : "grid grid-cols-1 gap-0 md:grid-cols-2"
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

        <div className="min-h-[280px] min-w-0 lg:col-span-4 lg:min-h-0">
          <EventTicker events={events} timezone={health?.timezone ?? null} />
        </div>
      </div>
    </div>
  );
}

function TelemetryCell({
  label,
  value,
  last,
  testId,
}: {
  label: string;
  value: string;
  last?: boolean;
  testId: string;
}) {
  return (
    <div
      className={`min-w-[6.5rem] flex-1 px-4 py-2 ${
        last ? "" : "border-r border-hairline"
      }`}
    >
      <div className="text-[10px] font-bold uppercase tracking-label text-muted">
        {label}
      </div>
      <div className="mt-0.5 text-lg text-ink" data-testid={testId}>
        {value}
      </div>
    </div>
  );
}
