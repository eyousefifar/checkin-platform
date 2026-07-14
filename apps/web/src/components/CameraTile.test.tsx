import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CameraSession } from "@/hooks/useCameraSessions";
import type { FaceDet } from "@/lib/types";

let cameraSession: CameraSession | undefined;

vi.mock("@/hooks/useCameraSessions", () => ({
  useCameraSession: () => cameraSession,
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

function stream(stop = vi.fn()): MediaStream {
  return { getTracks: () => [{ stop }] } as unknown as MediaStream;
}

describe("CameraTile persistent stream attachment", () => {
  beforeEach(() => {
    cameraSession = undefined;
    class RO {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", RO);
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("waits for a health-provided path and session", () => {
    render(
      <CameraTile cameraId="cam_in" name="Entrance" direction="IN" faces={[]} />,
    );
    expect(screen.getByText(/Health retrying/i)).toBeTruthy();
    expect(screen.getByTestId("camera-capture-badge").textContent).toBe("UNKNOWN");
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("CONNECTING");
  });

  it("stays connecting after session success until the video playing event", async () => {
    const existing = stream();
    cameraSession = {
      path: "demo",
      stream: existing,
      state: "connected",
      error: null,
    };
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

    const video = screen.getByTestId("camera-video") as HTMLVideoElement;
    expect(video.srcObject).toBe(existing);
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("CONNECTING");
    await act(async () => fireEvent(video, new Event("playing")));
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("VIDEO LIVE");
    expect(screen.getByTestId("camera-capture-badge").textContent).toBe("ONLINE");
  });

  it("shows provider errors and recovers only after playing", async () => {
    cameraSession = {
      path: "demo",
      stream: null,
      state: "error",
      error: "WHEP 503",
    };
    const { rerender } = render(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
        webrtcPath="demo"
      />,
    );
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("VIDEO ERROR");
    expect(screen.getByTestId("video-status-text").textContent).toContain("WHEP 503");

    cameraSession = {
      path: "demo",
      stream: stream(),
      state: "connected",
      error: null,
    };
    rerender(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
        webrtcPath="demo"
      />,
    );
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("CONNECTING");
    await act(async () =>
      fireEvent(screen.getByTestId("camera-video"), new Event("playing")),
    );
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("VIDEO LIVE");
  });

  it("detaches on unmount without stopping provider-owned tracks", () => {
    const stop = vi.fn();
    cameraSession = {
      path: "demo",
      stream: stream(stop),
      state: "connected",
      error: null,
    };
    const { unmount } = render(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
        webrtcPath="demo"
      />,
    );
    const video = screen.getByTestId("camera-video") as HTMLVideoElement;
    unmount();
    expect(video.srcObject).toBeNull();
    expect(stop).not.toHaveBeenCalled();
  });

  it("leaves playing on stalled and recovers on the next playing event", async () => {
    cameraSession = {
      path: "demo",
      stream: stream(),
      state: "connected",
      error: null,
    };
    render(
      <CameraTile
        cameraId="cam_in"
        name="Entrance"
        direction="IN"
        faces={[]}
        webrtcPath="demo"
      />,
    );
    const video = screen.getByTestId("camera-video");
    await act(async () => fireEvent(video, new Event("playing")));
    fireEvent(video, new Event("stalled"));
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("CONNECTING");
    fireEvent(video, new Event("playing"));
    expect(screen.getByTestId("browser-video-badge").textContent).toBe("VIDEO LIVE");
  });

  it("never derives camera capture status from browser playback", async () => {
    cameraSession = {
      path: "demo",
      stream: stream(),
      state: "connected",
      error: null,
    };
    for (const test of [
      { online: undefined, want: "UNKNOWN" },
      { online: false, want: "OFFLINE" },
      { online: true, want: "ONLINE" },
    ]) {
      const { unmount } = render(
        <CameraTile
          cameraId="cam_in"
          name="Entrance"
          direction="IN"
          faces={[]}
          webrtcPath="demo"
          online={test.online}
        />,
      );
      await act(async () =>
        fireEvent(screen.getByTestId("camera-video"), new Event("playing")),
      );
      expect(screen.getByTestId("camera-capture-badge").textContent).toBe(test.want);
      unmount();
    }
  });
});
