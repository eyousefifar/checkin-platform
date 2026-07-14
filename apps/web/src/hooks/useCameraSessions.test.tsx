import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WhepHandle } from "@/lib/whep";

const connectWhep = vi.hoisted(() => vi.fn());
const health = vi.hoisted(() => ({
  data: {
    cameras: [
      {
        id: "cam_in",
        name: "Entrance",
        direction: "in",
        enabled: true,
        webrtc_path: "demo",
      },
    ],
  },
}));

vi.mock("@/lib/whep", () => ({
  connectWhep: (...args: unknown[]) => connectWhep(...args),
  whepUrl: (base: string, path: string) => `${base}/${path}/whep`,
}));

vi.mock("./useHealth", () => ({
  useHealth: () => ({ data: health.data, loading: false, error: null, refresh: vi.fn() }),
}));

import { CameraSessionsProvider, useCameraSession } from "./useCameraSessions";

class TestStream extends EventTarget {
  tracks: MediaStreamTrack[] = [];
  getTracks() {
    return this.tracks;
  }
}

function makeHandle() {
  const stream = new TestStream() as unknown as MediaStream;
  const pc = {
    connectionState: "connected" as RTCPeerConnectionState,
    onconnectionstatechange: null as ((this: RTCPeerConnection, ev: Event) => void) | null,
  } as unknown as RTCPeerConnection;
  const close = vi.fn();
  return { pc, stream, close } satisfies WhepHandle;
}

function Probe() {
  const session = useCameraSession("cam_in");
  return <div data-testid="probe">{session?.state ?? "missing"}</div>;
}

function Host({ dashboard }: { dashboard: boolean }) {
  return (
    <CameraSessionsProvider>
      {dashboard ? <Probe /> : <div data-testid="other-page">Other page</div>}
    </CameraSessionsProvider>
  );
}

describe("persistent camera sessions", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    connectWhep.mockReset();
    health.data = {
      cameras: [
        {
          id: "cam_in",
          name: "Entrance",
          direction: "in",
          enabled: true,
          webrtc_path: "demo",
        },
      ],
    };
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("keeps one WHEP session while route content unmounts and reuses its stream", async () => {
    const handle = makeHandle();
    connectWhep.mockResolvedValue(handle);
    const { rerender } = render(<Host dashboard />);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(connectWhep).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("probe").textContent).toBe("connected");

    rerender(<Host dashboard={false} />);
    expect(handle.close).not.toHaveBeenCalled();
    rerender(<Host dashboard />);
    expect(connectWhep).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("probe").textContent).toBe("connected");
  });

  it("gives disconnected one second before closing, then retries after 500ms", async () => {
    const first = makeHandle();
    const second = makeHandle();
    connectWhep.mockResolvedValueOnce(first).mockResolvedValueOnce(second);
    render(<Host dashboard />);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    Object.assign(first.pc, { connectionState: "disconnected" });
    act(() => first.pc.onconnectionstatechange?.call(first.pc, new Event("change")));
    await act(async () => vi.advanceTimersByTimeAsync(999));
    expect(first.close).not.toHaveBeenCalled();

    Object.assign(first.pc, { connectionState: "connected" });
    act(() => first.pc.onconnectionstatechange?.call(first.pc, new Event("change")));
    await act(async () => vi.advanceTimersByTimeAsync(1000));
    expect(first.close).not.toHaveBeenCalled();

    Object.assign(first.pc, { connectionState: "disconnected" });
    act(() => first.pc.onconnectionstatechange?.call(first.pc, new Event("change")));
    await act(async () => vi.advanceTimersByTimeAsync(1000));
    expect(first.close).toHaveBeenCalledTimes(1);
    expect(connectWhep).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
      await Promise.resolve();
    });
    expect(connectWhep).toHaveBeenCalledTimes(2);
  });

  it("closes exactly once on path change and root unmount", async () => {
    const first = makeHandle();
    const second = makeHandle();
    connectWhep.mockResolvedValueOnce(first).mockResolvedValueOnce(second);
    const { rerender, unmount } = render(<Host dashboard />);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    health.data = {
      cameras: [
        {
          id: "cam_in",
          name: "Entrance",
          direction: "in",
          enabled: true,
          webrtc_path: "changed",
        },
      ],
    };
    rerender(<Host dashboard />);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(first.close).toHaveBeenCalledTimes(1);
    expect(connectWhep).toHaveBeenCalledTimes(2);

    unmount();
    expect(second.close).toHaveBeenCalledTimes(1);
  });
});
