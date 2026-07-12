import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FaceDet } from "@/lib/types";
import type { WhepHandle } from "@/lib/whep";

const connectWhep = vi.hoisted(() => vi.fn());

vi.mock("@/lib/whep", () => ({
  whepUrl: (base: string, path: string) => `${base}/${path}/whep`,
  connectWhep: (...args: unknown[]) => connectWhep(...args),
}));

vi.mock("./FaceHudCanvas", () => ({
  FaceHudCanvas: () => <canvas data-testid="hud" />,
}));

import { CameraTile } from "./CameraTile";

function face(label = "A"): FaceDet {
  return {
    track_id: 1,
    bbox: [0.1, 0.1, 0.2, 0.2],
    label,
    score: 0.9,
    quality_ok: true,
    state: "tracked",
  };
}

function mockHandle(overrides: Partial<RTCPeerConnection> = {}): WhepHandle {
  const pc = {
    connectionState: "connected",
    onconnectionstatechange: null as ((this: RTCPeerConnection, ev: Event) => void) | null,
    close: vi.fn(),
    ...overrides,
  } as unknown as RTCPeerConnection;
  return {
    pc,
    close: vi.fn(() => {
      pc.close();
    }),
  };
}

describe("CameraTile WHEP lifecycle", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    connectWhep.mockReset();
    class RO {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", RO);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("does not connect WHEP without a health-provided path", async () => {
    render(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
      />,
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(connectWhep).not.toHaveBeenCalled();
    expect(screen.getByText(/Health retrying/i)).toBeTruthy();
    expect(screen.getByTestId("camera-capture-badge").textContent).toBe("UNKNOWN");
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("CONNECTING");
  });

  it("stays connecting after SDP success until playing event", async () => {
    const handle = mockHandle();
    connectWhep.mockResolvedValue(handle);

    render(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[face()]}
        webrtcPath="demo"
        online={true}
      />,
    );

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(connectWhep).toHaveBeenCalledTimes(1);
    expect(connectWhep.mock.calls[0][0]).toMatch(/\/demo\/whep$/);
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("CONNECTING");
    // Camera capture stays ONLINE from WS; video does not rewrite it.
    expect(screen.getByTestId("camera-capture-badge").textContent).toBe("ONLINE");
    expect(screen.getByTestId("camera-capture-badge").getAttribute("data-camera-status")).toBe(
      "online",
    );

    const video = screen.getByTestId("camera-video");
    await act(async () => {
      fireEvent(video, new Event("playing"));
    });
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("VIDEO LIVE");
    expect(screen.getByTestId("camera-capture-badge").textContent).toBe("ONLINE");
  });

  it("retries on WHEP failure with one owned timer and keeps error visible", async () => {
    connectWhep
      .mockRejectedValueOnce(new Error("WHEP 503"))
      .mockResolvedValueOnce(mockHandle());

    render(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
        webrtcPath="cam_in"
      />,
    );

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(connectWhep).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("VIDEO ERROR");
    expect(screen.getByTestId("video-status-text").textContent).toMatch(/WHEP 503/);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(connectWhep).toHaveBeenCalledTimes(2);
  });

  it("leaves playing on stalled and recovers on next playing", async () => {
    connectWhep.mockResolvedValue(mockHandle());
    render(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
        webrtcPath="demo"
      />,
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    const video = screen.getByTestId("camera-video");
    await act(async () => {
      fireEvent(video, new Event("playing"));
    });
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("VIDEO LIVE");

    await act(async () => {
      fireEvent(video, new Event("stalled"));
    });
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("CONNECTING");

    await act(async () => {
      fireEvent(video, new Event("playing"));
    });
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("VIDEO LIVE");
  });

  it("closes handle and clears timers on path change and unmount", async () => {
    const handle1 = mockHandle();
    const handle2 = mockHandle();
    connectWhep.mockResolvedValueOnce(handle1).mockResolvedValueOnce(handle2);

    const { rerender, unmount } = render(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
        webrtcPath="demo"
      />,
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(connectWhep).toHaveBeenCalledTimes(1);

    rerender(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
        webrtcPath="cam_in"
      />,
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(handle1.close).toHaveBeenCalled();
    expect(connectWhep).toHaveBeenCalledTimes(2);
    expect(connectWhep.mock.calls[1][0]).toMatch(/\/cam_in\/whep$/);

    unmount();
    expect(handle2.close).toHaveBeenCalled();
  });

  it("camera × video matrix never false-greens capture from video", async () => {
    connectWhep.mockResolvedValue(mockHandle());
    const cases: Array<{ online?: boolean; want: string }> = [
      { online: undefined, want: "UNKNOWN" },
      { online: false, want: "OFFLINE" },
      { online: true, want: "ONLINE" },
    ];
    for (const c of cases) {
      const { unmount } = render(
        <CameraTile
          cameraId="cam_in"
          name="Entrance"
          direction="IN"
          faces={[]}
          webrtcPath="demo"
          online={c.online}
        />,
      );
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      const video = screen.getByTestId("camera-video");
      await act(async () => {
        fireEvent(video, new Event("playing"));
      });
      expect(screen.getByTestId("browser-video-badge").textContent).toBe("VIDEO LIVE");
      expect(screen.getByTestId("camera-capture-badge").textContent).toBe(c.want);
      unmount();
    }
  });

  it("contains no HLS fallback path", () => {
    // Static guarantee also covered by source grep in plan verify step.
    render(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
        webrtcPath="demo"
      />,
    );
    expect(screen.queryByText(/HLS/i)).toBeNull();
  });
});
