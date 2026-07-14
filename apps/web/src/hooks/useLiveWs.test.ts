import { act, renderHook } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DETECTION_FRESHNESS_MS, LiveWsProvider, useLiveWs } from "./useLiveWs";

const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(LiveWsProvider, null, children);

type Handler = ((ev?: { data?: string }) => void) | null;

class FakeWebSocket {
  static OPEN = 1;
  static instances: FakeWebSocket[] = [];
  static shouldThrowOnConstruct = false;
  /** When false, sockets stay pending (no onopen) for backoff tests. */
  static autoOpen = true;

  url: string;
  readyState = FakeWebSocket.OPEN;
  onopen: Handler = null;
  onclose: Handler = null;
  onerror: Handler = null;
  onmessage: Handler = null;
  closed = false;

  constructor(url: string) {
    if (FakeWebSocket.shouldThrowOnConstruct) {
      throw new Error("construct failed");
    }
    this.url = url;
    FakeWebSocket.instances.push(this);
    if (FakeWebSocket.autoOpen) {
      queueMicrotask(() => {
        if (!this.closed) this.onopen?.({} as never);
      });
    }
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    this.readyState = 3;
    this.onclose?.({} as never);
  }

  emit(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) });
  }
}

describe("useLiveWs", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    FakeWebSocket.instances = [];
    FakeWebSocket.shouldThrowOnConstruct = false;
    FakeWebSocket.autoOpen = true;
    vi.stubGlobal("WebSocket", FakeWebSocket as unknown as typeof WebSocket);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("connects and tracks transport-only connected flag", async () => {
    const { result } = renderHook(() => useLiveWs(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.connected).toBe(true);
    expect(result.current.cameraOnline["cam_in"]).toBeUndefined();
    expect(FakeWebSocket.instances).toHaveLength(1);
  });

  it("reconnects after close with capped exponential backoff", async () => {
    // Keep sockets from opening so backoff is not reset on onopen.
    FakeWebSocket.autoOpen = false;
    renderHook(() => useLiveWs(), { wrapper });
    expect(FakeWebSocket.instances).toHaveLength(1);
    await act(async () => {
      FakeWebSocket.instances[0].close();
    });

    // First retry after 500ms (500 * 2^0)
    await act(async () => {
      vi.advanceTimersByTime(499);
      await Promise.resolve();
    });
    expect(FakeWebSocket.instances).toHaveLength(1);
    await act(async () => {
      vi.advanceTimersByTime(1);
      await Promise.resolve();
    });
    expect(FakeWebSocket.instances).toHaveLength(2);

    await act(async () => {
      FakeWebSocket.instances[1].close();
    });
    // Second retry after 1000ms (500 * 2^1) — not before.
    await act(async () => {
      vi.advanceTimersByTime(999);
      await Promise.resolve();
    });
    expect(FakeWebSocket.instances).toHaveLength(2);
    await act(async () => {
      vi.advanceTimersByTime(1);
      await Promise.resolve();
    });
    expect(FakeWebSocket.instances).toHaveLength(3);
    expect(FakeWebSocket.instances.filter((s) => !s.closed)).toHaveLength(1);
  });

  it("unmount never schedules reconnect and leaves no live socket timer", async () => {
    const { unmount } = renderHook(() => useLiveWs(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    const sock = FakeWebSocket.instances[0];
    unmount();
    const count = FakeWebSocket.instances.length;
    // Trigger close after unmount — stopped flag must block reconnect.
    await act(async () => {
      sock.close();
      vi.advanceTimersByTime(20000);
      await Promise.resolve();
    });
    expect(FakeWebSocket.instances.length).toBe(count);
  });

  it("construction failure schedules reconnect only while mounted", async () => {
    FakeWebSocket.shouldThrowOnConstruct = true;
    const { unmount } = renderHook(() => useLiveWs(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    FakeWebSocket.shouldThrowOnConstruct = false;
    await act(async () => {
      vi.advanceTimersByTime(2000);
      await Promise.resolve();
    });
    expect(FakeWebSocket.instances.length).toBeGreaterThanOrEqual(1);
    const afterConnect = FakeWebSocket.instances.length;
    unmount();
    await act(async () => {
      vi.advanceTimersByTime(20000);
      await Promise.resolve();
    });
    expect(FakeWebSocket.instances.length).toBe(afterConnect);
  });

  it("preserves unknown camera status until camera_status arrives", async () => {
    const { result } = renderHook(() => useLiveWs(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.cameraOnline["cam_in"]).toBeUndefined();
    await act(async () => {
      FakeWebSocket.instances[0].emit({
        type: "camera_status",
        camera_id: "cam_in",
        online: true,
      });
    });
    expect(result.current.cameraOnline["cam_in"]).toBe(true);
  });

  it("clears detections on offline, close, and 500ms staleness", async () => {
    const { result } = renderHook(() => useLiveWs(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    const sock = FakeWebSocket.instances[0];
    await act(async () => {
      sock.emit({
        type: "detections",
        camera_id: "cam_in",
        ts: 1,
        frame_w: 640,
        frame_h: 360,
        faces: [
          {
            track_id: 1,
            bbox: [0, 0, 10, 10],
            label: "A",
            score: 0.9,
            quality_ok: true,
            state: "ok",
          },
        ],
      });
    });
    expect(result.current.detections["cam_in"]).toHaveLength(1);

    // Refresh keeps faces.
    await act(async () => {
      vi.advanceTimersByTime(400);
      sock.emit({
        type: "detections",
        camera_id: "cam_in",
        ts: 2,
        frame_w: 640,
        frame_h: 360,
        faces: [
          {
            track_id: 1,
            bbox: [0, 0, 10, 10],
            label: "A",
            score: 0.9,
            quality_ok: true,
            state: "ok",
          },
        ],
      });
    });
    expect(result.current.detections["cam_in"]).toHaveLength(1);

    // Staleness expiry.
    await act(async () => {
      vi.advanceTimersByTime(DETECTION_FRESHNESS_MS + 50);
    });
    expect(result.current.detections["cam_in"]).toBeUndefined();

    // Re-seed then explicit offline.
    await act(async () => {
      sock.emit({
        type: "detections",
        camera_id: "cam_in",
        ts: 3,
        frame_w: 640,
        frame_h: 360,
        faces: [
          {
            track_id: 2,
            bbox: [0, 0, 10, 10],
            label: "B",
            score: 0.8,
            quality_ok: true,
            state: "ok",
          },
        ],
      });
      sock.emit({
        type: "camera_status",
        camera_id: "cam_in",
        online: false,
      });
    });
    expect(result.current.cameraOnline["cam_in"]).toBe(false);
    expect(result.current.detections["cam_in"]).toBeUndefined();

    // Close clears all.
    await act(async () => {
      sock.emit({
        type: "detections",
        camera_id: "cam_out",
        ts: 4,
        frame_w: 640,
        frame_h: 360,
        faces: [
          {
            track_id: 3,
            bbox: [0, 0, 10, 10],
            label: "C",
            score: 0.7,
            quality_ok: true,
            state: "ok",
          },
        ],
      });
    });
    expect(result.current.detections["cam_out"]).toHaveLength(1);
    await act(async () => {
      sock.close();
    });
    expect(result.current.detections).toEqual({});
  });

  it("ignores malformed messages without crashing", async () => {
    const { result } = renderHook(() => useLiveWs(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    await act(async () => {
      FakeWebSocket.instances[0].onmessage?.({ data: "not-json{" } as never);
    });
    expect(result.current.connected).toBe(true);
  });
});
