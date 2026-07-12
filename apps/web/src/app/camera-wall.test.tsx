import { act, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FaceDet, HealthCamera, HealthResponse, MetricsMsg } from "@/lib/types";

const liveState = vi.hoisted(() => ({
  connected: true,
  detections: {} as Record<string, FaceDet[]>,
  events: [] as { event_id: number }[],
  metrics: null as MetricsMsg | null,
  cameraOnline: {} as Record<string, boolean | undefined>,
}));

const healthState = vi.hoisted(() => ({
  data: null as HealthResponse | null,
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

const cameraRenders = vi.hoisted(() => ({
  tiles: [] as Record<string, unknown>[],
}));

vi.mock("@/components/CameraTile", () => ({
  CameraTile: (props: Record<string, unknown>) => {
    cameraRenders.tiles.push(props);
    return (
      <div
        data-testid="camera-tile"
        data-camera-id={String(props.cameraId)}
        data-online={
          props.online === undefined
            ? "unknown"
            : props.online
              ? "online"
              : "offline"
        }
        data-webrtc-path={
          typeof props.webrtcPath === "string" ? props.webrtcPath : ""
        }
        data-direction={String(props.direction ?? "")}
        data-fps={props.fps == null ? "" : String(props.fps)}
        data-face-count={Array.isArray(props.faces) ? props.faces.length : 0}
      >
        {String(props.name)}
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

function makeHealth(cameras: HealthCamera[]): HealthResponse {
  return {
    status: "ok",
    timezone: "Asia/Tehran",
    vision_ready: true,
    vision_provider: "mock",
    gallery_size: 0,
    cameras,
    media: {
      mediamtx_running: true,
      transcoder_running: true,
      publication: "live",
      source_mode: "external",
      preferred_webrtc_path: null,
      last_error: null,
    },
  };
}

function cam(
  partial: Partial<HealthCamera> & Pick<HealthCamera, "id">,
): HealthCamera {
  return {
    name: partial.name ?? partial.id,
    direction: partial.direction ?? "in",
    enabled: partial.enabled ?? true,
    webrtc_path: partial.webrtc_path ?? partial.id,
    ...partial,
  };
}

/** Public HealthCamera must never carry source URLs. */
function assertPublicCamera(c: HealthCamera) {
  const keys = Object.keys(c).sort();
  expect(keys).toEqual(
    ["direction", "enabled", "id", "name", "webrtc_path"].sort(),
  );
  expect(c).not.toHaveProperty("rtsp_url");
  expect(JSON.stringify(c)).not.toMatch(/rtsp:\/\//i);
}

describe("camera wall from health", () => {
  beforeEach(() => {
    liveState.connected = true;
    liveState.detections = {};
    liveState.events = [];
    liveState.metrics = null;
    liveState.cameraOnline = {};
    healthState.data = null;
    healthState.loading = true;
    healthState.error = null;
    cameraRenders.tiles = [];
  });

  it("HealthCamera public projection excludes RTSP source fields", () => {
    const sample = cam({
      id: "cam_in",
      name: "Entrance",
      direction: "in",
      enabled: true,
      webrtc_path: "cam_in_h264",
    });
    assertPublicCamera(sample);
    const health = makeHealth([sample]);
    for (const c of health.cameras) assertPublicCamera(c);
  });

  it("while health retries, shows health state and starts no WHEP tiles", () => {
    healthState.loading = true;
    healthState.error = "network down";
    healthState.data = null;
    render(<DashboardPage />);
    expect(screen.getByTestId("health-retrying").textContent).toMatch(
      /retrying/i,
    );
    expect(screen.getByTestId("health-camera-state").textContent).toMatch(
      /Health retrying/i,
    );
    expect(screen.queryByTestId("camera-tile")).toBeNull();
    expect(screen.queryByTestId("camera-wall")).toBeNull();
    expect(cameraRenders.tiles).toHaveLength(0);
  });

  it("recovers from health failure and maps path without cam_in hardcode", async () => {
    healthState.loading = true;
    healthState.error = "boom";
    healthState.data = null;
    const { rerender } = render(<DashboardPage />);
    expect(screen.queryByTestId("camera-tile")).toBeNull();

    await act(async () => {
      healthState.loading = false;
      healthState.error = null;
      healthState.data = makeHealth([
        cam({
          id: "entrance_a",
          name: "Dock A",
          direction: "IN",
          webrtc_path: "pub_a",
        }),
      ]);
      rerender(<DashboardPage />);
    });

    expect(screen.queryByTestId("health-camera-state")).toBeNull();
    const tile = screen.getByTestId("camera-tile");
    expect(tile.getAttribute("data-camera-id")).toBe("entrance_a");
    expect(tile.getAttribute("data-webrtc-path")).toBe("pub_a");
    expect(tile.textContent).toContain("Dock A");
    // No hardcoded cam_in fallback.
    expect(cameraRenders.tiles[0]?.cameraId).toBe("entrance_a");
    expect(cameraRenders.tiles[0]?.cameraId).not.toBe("cam_in");
  });

  it("zero enabled cameras shows deployment guidance, not a demo path", () => {
    healthState.loading = false;
    healthState.data = makeHealth([
      cam({ id: "cam_in", enabled: false, webrtc_path: "demo" }),
    ]);
    render(<DashboardPage />);
    const empty = screen.getByTestId("no-enabled-cameras");
    expect(empty.textContent).toMatch(/No enabled cameras/i);
    expect(empty.textContent).toMatch(/deployment/i);
    expect(screen.queryByTestId("camera-tile")).toBeNull();
    expect(cameraRenders.tiles).toHaveLength(0);
  });

  it("one enabled camera: single tile with keyed status, FPS, faces, path", () => {
    const face: FaceDet = {
      track_id: 1,
      bbox: [0, 0, 10, 10],
      label: "A",
      score: 0.9,
      quality_ok: true,
      state: "matched",
    };
    healthState.loading = false;
    healthState.data = makeHealth([
      cam({
        id: "cam_in",
        name: "Entrance",
        direction: "in",
        webrtc_path: "cam_in_h264",
      }),
      cam({ id: "cam_out", enabled: false, webrtc_path: "cam_out" }),
    ]);
    liveState.cameraOnline = { cam_in: true, cam_out: false };
    liveState.detections = { cam_in: [face], cam_out: [face, face] };
    liveState.metrics = {
      type: "metrics",
      cameras_online: 1,
      present_count: 0,
      events_today: 0,
      vision_fps: { cam_in: 4.5, cam_out: 9.9 },
    };

    render(<DashboardPage />);
    const wall = screen.getByTestId("camera-wall");
    expect(wall.getAttribute("data-camera-count")).toBe("1");
    const tiles = screen.getAllByTestId("camera-tile");
    expect(tiles).toHaveLength(1);
    expect(tiles[0].getAttribute("data-camera-id")).toBe("cam_in");
    expect(tiles[0].getAttribute("data-online")).toBe("online");
    expect(tiles[0].getAttribute("data-webrtc-path")).toBe("cam_in_h264");
    expect(tiles[0].getAttribute("data-direction")).toBe("in");
    expect(tiles[0].getAttribute("data-fps")).toBe("4.5");
    expect(tiles[0].getAttribute("data-face-count")).toBe("1");
    expect(cameraRenders.tiles[0]?.faces).toEqual([face]);
  });

  it("two enabled cameras: grid tiles with non-cross-wired props", () => {
    const faceIn: FaceDet = {
      track_id: 1,
      bbox: [0, 0, 1, 1],
      label: "IN",
      score: 0.8,
      quality_ok: true,
      state: "matched",
    };
    const faceOut: FaceDet = {
      track_id: 2,
      bbox: [0, 0, 2, 2],
      label: "OUT",
      score: 0.7,
      quality_ok: true,
      state: "matched",
    };
    healthState.loading = false;
    healthState.data = makeHealth([
      cam({
        id: "cam_in",
        name: "Entrance",
        direction: "in",
        webrtc_path: "cam_in_h264",
      }),
      cam({
        id: "cam_out",
        name: "Exit",
        direction: "out",
        webrtc_path: "cam_out",
      }),
    ]);
    liveState.cameraOnline = { cam_in: true, cam_out: false };
    liveState.detections = { cam_in: [faceIn], cam_out: [faceOut] };
    liveState.metrics = {
      type: "metrics",
      cameras_online: 1,
      present_count: 1,
      events_today: 2,
      vision_fps: { cam_in: 5, cam_out: 3 },
    };

    render(<DashboardPage />);
    const wall = screen.getByTestId("camera-wall");
    expect(wall.getAttribute("data-camera-count")).toBe("2");
    const tiles = screen.getAllByTestId("camera-tile");
    expect(tiles).toHaveLength(2);

    const byId = Object.fromEntries(
      cameraRenders.tiles.map((t) => [t.cameraId as string, t]),
    );
    expect(byId.cam_in).toMatchObject({
      name: "Entrance",
      direction: "in",
      online: true,
      webrtcPath: "cam_in_h264",
      fps: 5,
    });
    expect(byId.cam_in.faces).toEqual([faceIn]);
    expect(byId.cam_out).toMatchObject({
      name: "Exit",
      direction: "out",
      online: false,
      webrtcPath: "cam_out",
      fps: 3,
    });
    expect(byId.cam_out.faces).toEqual([faceOut]);

    // DOM order follows health list order.
    expect(tiles[0].getAttribute("data-camera-id")).toBe("cam_in");
    expect(tiles[1].getAttribute("data-camera-id")).toBe("cam_out");
  });

  it("three enabled cameras: caps at two and shows operator warning", () => {
    healthState.loading = false;
    healthState.data = makeHealth([
      cam({ id: "a", name: "A", webrtc_path: "pa" }),
      cam({ id: "b", name: "B", webrtc_path: "pb" }),
      cam({ id: "c", name: "C", webrtc_path: "pc" }),
    ]);
    render(<DashboardPage />);
    expect(screen.getByTestId("camera-wall").getAttribute("data-camera-count")).toBe(
      "2",
    );
    const tiles = screen.getAllByTestId("camera-tile");
    expect(tiles).toHaveLength(2);
    expect(tiles.map((t) => t.getAttribute("data-camera-id"))).toEqual([
      "a",
      "b",
    ]);
    expect(screen.queryByText("C")).toBeNull();
    const warn = screen.getByTestId("camera-cap-warning");
    expect(warn.textContent).toMatch(/supports two/i);
    expect(within(warn).getByText(/3 enabled/i)).toBeTruthy();
    expect(cameraRenders.tiles).toHaveLength(2);
  });

  it("does not hardcode cam_in when health lists only cam_out", () => {
    healthState.loading = false;
    healthState.data = makeHealth([
      cam({
        id: "cam_out",
        name: "Exit only",
        direction: "out",
        webrtc_path: "exit_path",
      }),
    ]);
    liveState.cameraOnline = { cam_out: true };
    liveState.metrics = {
      type: "metrics",
      cameras_online: 1,
      present_count: 0,
      events_today: 0,
      vision_fps: { cam_out: 6.2 },
    };
    render(<DashboardPage />);
    const tile = screen.getByTestId("camera-tile");
    expect(tile.getAttribute("data-camera-id")).toBe("cam_out");
    expect(tile.getAttribute("data-webrtc-path")).toBe("exit_path");
    expect(cameraRenders.tiles.some((t) => t.cameraId === "cam_in")).toBe(false);
  });
});
