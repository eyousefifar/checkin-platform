import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MetricsMsg } from "@/lib/types";

const liveState = vi.hoisted(() => ({
  connected: true,
  detections: {} as Record<string, never>,
  events: [] as { event_id: number }[],
  metrics: null as MetricsMsg | null,
  cameraOnline: {} as Record<string, boolean | undefined>,
}));

vi.mock("@/hooks/useLiveWs", () => ({
  useLiveWs: () => liveState,
}));

vi.mock("@/components/CameraTile", () => ({
  CameraTile: (props: {
    online?: boolean;
    name: string;
  }) => (
    <div
      data-testid="camera-tile"
      data-online={
        props.online === undefined ? "unknown" : props.online ? "online" : "offline"
      }
    >
      {props.name}
    </div>
  ),
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

describe("DashboardPage live state", () => {
  beforeEach(() => {
    liveState.connected = true;
    liveState.detections = {};
    liveState.events = [];
    liveState.metrics = null;
    liveState.cameraOnline = {};
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        json: async () => ({ cameras: [{ id: "cam_in", webrtc_path: "demo" }] }),
      }),
    );
  });

  it("shows WS linked with camera unknown without a green online state", () => {
    render(<DashboardPage />);
    expect(screen.getByText("WS linked")).toBeTruthy();
    const tile = screen.getByTestId("camera-tile");
    expect(tile.getAttribute("data-online")).toBe("unknown");
    // Cameras metric must not infer 1 from WS connectivity alone.
    expect(screen.getByTestId("metric-Cameras").textContent).toBe("0");
  });

  it("renders camera online only after explicit status", () => {
    liveState.cameraOnline = { cam_in: true };
    render(<DashboardPage />);
    expect(screen.getByTestId("camera-tile").getAttribute("data-online")).toBe(
      "online",
    );
  });

  it("shows em dash for unavailable metrics, not invented zero", () => {
    liveState.metrics = null;
    liveState.events = [{ event_id: 1 }, { event_id: 2 }];
    render(<DashboardPage />);
    expect(screen.getByTestId("metric-Present").textContent).toBe("—");
    // Must not fall back to in-memory event ticker length.
    expect(screen.getByTestId("metric-Events today").textContent).toBe("—");
  });

  it("renders real persisted zero distinctly from unavailable", () => {
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
});
