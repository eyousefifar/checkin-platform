import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MetricsMsg } from "@/lib/types";

const liveState = vi.hoisted(() => ({
  connected: true,
  detections: {} as Record<string, never>,
  events: [] as { event_id: number }[],
  metrics: null as MetricsMsg | null,
  cameraOnline: {} as Record<string, boolean | undefined>,
}));

const healthState = vi.hoisted(() => ({
  data: null as null | {
    status: string;
    timezone: string;
    vision_ready: boolean;
    vision_provider: string;
    gallery_size: number;
    cameras: { id: string; name: string; direction: string; enabled: boolean; webrtc_path: string }[];
    media: {
      mediamtx_running: boolean;
      transcoder_running: boolean;
      publication: string;
      source_mode: null;
      preferred_webrtc_path: null;
      last_error: null;
    };
  },
  loading: true,
  error: null as string | null,
  refresh: () => {},
}));

vi.mock("@/hooks/useLiveWs", () => ({
  useLiveWs: () => liveState,
}));

vi.mock("@/hooks/useHealth", () => ({
  useHealth: () => healthState,
}));

const cameraProps = vi.hoisted(() => ({ last: null as null | Record<string, unknown> }));

vi.mock("@/components/CameraTile", () => ({
  CameraTile: (props: {
    online?: boolean;
    name: string;
    webrtcPath?: string;
  }) => {
    cameraProps.last = props as unknown as Record<string, unknown>;
    return (
      <div
        data-testid="camera-tile"
        data-online={
          props.online === undefined ? "unknown" : props.online ? "online" : "offline"
        }
        data-webrtc-path={props.webrtcPath ?? ""}
      >
        {props.name}
      </div>
    );
  },
}));

vi.mock("@/components/EventTicker", () => ({
  EventTicker: () => <div data-testid="events" />,
}));

vi.mock("@/components/MetricPill", () => ({
  MetricPill: ({ label, value }: { label: string; value: string | number }) => (
    <div data-testid={`metric-${label}`}>{String(value)}</div>
  ),
}));

import DashboardPage from "./page";

function healthWithPath(path: string) {
  return {
    status: "ok",
    timezone: "UTC",
    vision_ready: true,
    vision_provider: "mock",
    gallery_size: 0,
    cameras: [
      {
        id: "cam_in",
        name: "Entrance",
        direction: "in",
        enabled: true,
        webrtc_path: path,
      },
    ],
    media: {
      mediamtx_running: true,
      transcoder_running: true,
      publication: "live",
      source_mode: null,
      preferred_webrtc_path: null,
      last_error: null,
    },
  };
}

describe("DashboardPage live state", () => {
  beforeEach(() => {
    liveState.connected = true;
    liveState.detections = {};
    liveState.events = [];
    liveState.metrics = null;
    liveState.cameraOnline = {};
    healthState.data = null;
    healthState.loading = true;
    healthState.error = null;
    cameraProps.last = null;
  });

  it("shows WS linked with camera unknown without a green online state", () => {
    healthState.data = healthWithPath("demo");
    healthState.loading = false;
    render(<DashboardPage />);
    expect(screen.getByText("WS linked")).toBeTruthy();
    const tile = screen.getByTestId("camera-tile");
    expect(tile.getAttribute("data-online")).toBe("unknown");
    // Cameras metric must not infer 1 from WS connectivity alone.
    expect(screen.getByTestId("metric-Cameras").textContent).toBe("0");
  });

  it("renders camera online only after explicit status", () => {
    healthState.data = healthWithPath("demo");
    healthState.loading = false;
    liveState.cameraOnline = { cam_in: true };
    render(<DashboardPage />);
    expect(screen.getByTestId("camera-tile").getAttribute("data-online")).toBe(
      "online",
    );
  });

  it("shows em dash for unavailable metrics, not invented zero", () => {
    healthState.data = healthWithPath("demo");
    healthState.loading = false;
    liveState.metrics = null;
    liveState.events = [{ event_id: 1 }, { event_id: 2 }];
    render(<DashboardPage />);
    expect(screen.getByTestId("metric-Present").textContent).toBe("—");
    // Must not fall back to in-memory event ticker length.
    expect(screen.getByTestId("metric-Events today").textContent).toBe("—");
  });

  it("renders real persisted zero distinctly from unavailable", () => {
    healthState.data = healthWithPath("demo");
    healthState.loading = false;
    liveState.metrics = {
      type: "metrics",
      cameras_online: 0,
      present_count: 0,
      events_today: 0,
      vision_fps: {},
    };
    render(<DashboardPage />);
    expect(screen.getByTestId("metric-Present").textContent).toBe("0");
    expect(screen.getByTestId("metric-Events today").textContent).toBe("0");
  });

  it("renders non-zero persisted metrics", () => {
    healthState.data = healthWithPath("demo");
    healthState.loading = false;
    liveState.metrics = {
      type: "metrics",
      cameras_online: 1,
      present_count: 3,
      events_today: 7,
      vision_fps: { cam_in: 4.5 },
    };
    render(<DashboardPage />);
    expect(screen.getByTestId("metric-Present").textContent).toBe("3");
    expect(screen.getByTestId("metric-Events today").textContent).toBe("7");
  });

  it("fails health once, recovers later, and passes returned path without reload", async () => {
    healthState.loading = true;
    healthState.error = "network down";
    healthState.data = null;

    const { rerender } = render(<DashboardPage />);
    expect(screen.getByTestId("health-retrying").textContent).toMatch(/retrying/i);
    // No camera tile / assumed WHEP path while health is unavailable.
    expect(screen.queryByTestId("camera-tile")).toBeNull();
    expect(screen.getByTestId("health-camera-state")).toBeTruthy();

    // Simulate useHealth recovery with a non-default path.
    await act(async () => {
      healthState.loading = false;
      healthState.error = null;
      healthState.data = healthWithPath("cam_in");
      rerender(<DashboardPage />);
    });

    await waitFor(() => {
      expect(screen.queryByTestId("health-retrying")).toBeNull();
    });
    expect(screen.getByTestId("camera-tile").getAttribute("data-webrtc-path")).toBe(
      "cam_in",
    );
    expect(cameraProps.last?.webrtcPath).toBe("cam_in");
  });
});
