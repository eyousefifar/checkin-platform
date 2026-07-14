import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GuidedFaceCapture, POSE_SLOTS } from "./GuidedFaceCapture";

const analyzeMock = vi.fn();

vi.mock("@/lib/api", () => ({
  analyzeEnrollmentFrame: (...args: unknown[]) => analyzeMock(...args),
}));

function mockStream() {
  const track = { stop: vi.fn() };
  return {
    getTracks: () => [track],
    track,
  } as unknown as MediaStream;
}

describe("GuidedFaceCapture", () => {
  let stream: MediaStream;

  beforeEach(() => {
    stream = mockStream();
    analyzeMock.mockReset();
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: vi.fn().mockResolvedValue(stream),
      },
    });
    // Canvas toBlob polyfill for jsdom
    HTMLCanvasElement.prototype.toBlob = function (cb) {
      cb(new Blob(["jpeg-bytes"], { type: "image/jpeg" }));
    };
    HTMLCanvasElement.prototype.getContext = vi.fn(() => ({
      drawImage: vi.fn(),
    })) as unknown as typeof HTMLCanvasElement.prototype.getContext;
    Object.defineProperty(HTMLVideoElement.prototype, "videoWidth", {
      configurable: true,
      get: () => 640,
    });
    Object.defineProperty(HTMLVideoElement.prototype, "videoHeight", {
      configurable: true,
      get: () => 480,
    });
    Object.defineProperty(HTMLVideoElement.prototype, "readyState", {
      configurable: true,
      get: () => 4,
    });
    HTMLVideoElement.prototype.play = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      writable: true,
      value: vi.fn((f: Blob) => `blob:test-${(f as File).name || "x"}`),
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      writable: true,
      value: vi.fn(),
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows permission denial UI when getUserMedia rejects", async () => {
    (navigator.mediaDevices.getUserMedia as ReturnType<typeof vi.fn>).mockRejectedValue(
      Object.assign(new Error("Permission denied by user"), {
        name: "NotAllowedError",
      }),
    );
    render(<GuidedFaceCapture onCapturedChange={vi.fn()} />);
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => {
      expect(screen.getByTestId("camera-permission-error")).toBeTruthy();
    });
    expect(screen.getByTestId("camera-permission-error").textContent).toMatch(
      /permission denied/i,
    );
  });

  it("stops media tracks on stop camera", async () => {
    render(<GuidedFaceCapture onCapturedChange={vi.fn()} />);
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => {
      expect(screen.getByTestId("stop-camera")).toBeTruthy();
    });
    fireEvent.click(screen.getByTestId("stop-camera"));
    const track = (stream as unknown as { track: { stop: ReturnType<typeof vi.fn> } })
      .track;
    expect(track.stop).toHaveBeenCalled();
  });

  it("requests a high-resolution front camera stream for enrollment", async () => {
    render(<GuidedFaceCapture onCapturedChange={vi.fn()} />);
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => expect(screen.getByTestId("stop-camera")).toBeTruthy());

    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({
      video: {
        facingMode: "user",
        width: { ideal: 1280 },
        height: { ideal: 720 },
      },
      audio: false,
    });
  });

  it("captures accepted center pose and progresses slots", async () => {
    const onChange = vi.fn();
    analyzeMock.mockResolvedValue({
      accepted: true,
      reason: null,
      bbox: [0.2, 0.2, 0.8, 0.8],
      yaw: 0,
      face_count: 1,
    });

    render(<GuidedFaceCapture onCapturedChange={onChange} />);
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => {
      expect(screen.getByTestId("stop-camera")).toBeTruthy();
    });

    await waitFor(
      () => {
        expect(screen.getByTestId("pose-slot-center").getAttribute("data-state")).toBe(
          "done",
        );
      },
      { timeout: 3000 },
    );
    expect(onChange).toHaveBeenCalled();
    const files = onChange.mock.calls.at(-1)?.[0] as File[];
    expect(files.length).toBeGreaterThanOrEqual(1);
    expect(files[0].name).toContain("center");
  });

  it("shows rejection guidance for no_face without capturing", async () => {
    const onChange = vi.fn();
    analyzeMock.mockResolvedValue({
      accepted: false,
      reason: "no_face",
      bbox: null,
      yaw: null,
      face_count: 0,
    });
    render(<GuidedFaceCapture onCapturedChange={onChange} />);
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => expect(screen.getByTestId("stop-camera")).toBeTruthy());
    await waitFor(
      () => {
        expect(screen.getByTestId("capture-status").textContent).toMatch(/no face/i);
      },
      { timeout: 3000 },
    );
    expect(screen.getByTestId("pose-slot-center").getAttribute("data-state")).not.toBe(
      "done",
    );
  });

  it("renders five pose slots", () => {
    render(<GuidedFaceCapture onCapturedChange={vi.fn()} />);
    expect(POSE_SLOTS).toHaveLength(5);
    for (const s of POSE_SLOTS) {
      expect(screen.getByTestId(`pose-slot-${s.id}`)).toBeTruthy();
    }
  });

  it("maps anatomical left/right to the unmirrored model yaw convention", () => {
    // The backend defines positive yaw as the subject turning left. The video is
    // mirrored only for display, so capture bins must not invert model yaw.
    const left = POSE_SLOTS.find((slot) => slot.id === "left");
    const right = POSE_SLOTS.find((slot) => slot.id === "right");
    expect(left?.yawMin).toBeGreaterThan(0);
    expect(right?.yawMax).toBeLessThan(0);
  });

  it("releases the camera when the parent disables capture", async () => {
    const { rerender } = render(
      <GuidedFaceCapture onCapturedChange={vi.fn()} disabled={false} />,
    );
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => expect(screen.getByTestId("stop-camera")).toBeTruthy());

    rerender(<GuidedFaceCapture onCapturedChange={vi.fn()} disabled />);

    const track = (stream as unknown as { track: { stop: ReturnType<typeof vi.fn> } })
      .track;
    await waitFor(() => expect(track.stop).toHaveBeenCalled());
  });

  it("stops a camera stream that resolves after capture was disabled", async () => {
    let resolveStream: (value: MediaStream) => void = () => {};
    const pending = new Promise<MediaStream>((resolve) => {
      resolveStream = resolve;
    });
    (navigator.mediaDevices.getUserMedia as ReturnType<typeof vi.fn>).mockReturnValue(
      pending,
    );
    const { rerender } = render(
      <GuidedFaceCapture onCapturedChange={vi.fn()} disabled={false} />,
    );

    fireEvent.click(screen.getByTestId("start-camera"));
    rerender(<GuidedFaceCapture onCapturedChange={vi.fn()} disabled />);
    await act(async () => resolveStream(stream));

    const track = (stream as unknown as { track: { stop: ReturnType<typeof vi.fn> } })
      .track;
    await waitFor(() => expect(track.stop).toHaveBeenCalled());
    expect(screen.queryByTestId("stop-camera")).toBeNull();
  });

  it("matches the bbox stage aspect ratio to a widescreen camera feed", async () => {
    Object.defineProperty(HTMLVideoElement.prototype, "videoWidth", {
      configurable: true,
      get: () => 1280,
    });
    Object.defineProperty(HTMLVideoElement.prototype, "videoHeight", {
      configurable: true,
      get: () => 720,
    });

    render(<GuidedFaceCapture onCapturedChange={vi.fn()} />);
    fireEvent.click(screen.getByTestId("start-camera"));

    await waitFor(() => {
      expect(screen.getByTestId("capture-stage").style.aspectRatio).toBe(
        String(1280 / 720),
      );
    });
  });

  it("allows retake of a captured slot", async () => {
    const onChange = vi.fn();
    analyzeMock.mockResolvedValue({
      accepted: true,
      reason: null,
      bbox: [0.2, 0.2, 0.8, 0.8],
      yaw: 0,
      face_count: 1,
    });
    render(<GuidedFaceCapture onCapturedChange={onChange} />);
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => expect(screen.getByTestId("stop-camera")).toBeTruthy());
    await waitFor(
      () => {
        expect(screen.getByTestId("retake-center")).toBeTruthy();
      },
      { timeout: 3000 },
    );
    fireEvent.click(screen.getByTestId("retake-center"));
    await waitFor(() => {
      expect(screen.getByTestId("pose-slot-center").getAttribute("data-state")).not.toBe(
        "done",
      );
    });
  });

  it("enforces single in-flight analyze (second tick skipped while pending)", async () => {
    let resolveAnalyze: (v: unknown) => void = () => {};
    analyzeMock.mockImplementation(
      () =>
        new Promise((r) => {
          resolveAnalyze = r;
        }),
    );
    render(<GuidedFaceCapture onCapturedChange={vi.fn()} />);
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => expect(screen.getByTestId("stop-camera")).toBeTruthy());

    await waitFor(
      () => {
        expect(analyzeMock.mock.calls.length).toBeGreaterThanOrEqual(1);
      },
      { timeout: 3000 },
    );
    const callsAfterFirst = analyzeMock.mock.calls.length;
    // Wait for another interval while first is still pending
    await act(async () => {
      await new Promise((r) => setTimeout(r, 500));
    });
    // Still only the original in-flight request(s) — not unbounded
    expect(analyzeMock.mock.calls.length).toBe(callsAfterFirst);

    await act(async () => {
      resolveAnalyze({
        accepted: false,
        reason: "no_face",
        bbox: null,
        yaw: null,
        face_count: 0,
      });
    });
  });

  it("does not let an aborted request unlock a newer preview request", async () => {
    let resolveFirst: (v: unknown) => void = () => {};
    analyzeMock
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirst = resolve;
          }),
      )
      .mockImplementation(() => new Promise(() => {}));

    render(<GuidedFaceCapture onCapturedChange={vi.fn()} />);
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => expect(analyzeMock).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByTestId("stop-camera"));
    fireEvent.click(screen.getByTestId("start-camera"));
    await waitFor(() => expect(analyzeMock).toHaveBeenCalledTimes(2));

    await act(async () => {
      resolveFirst({
        accepted: false,
        reason: "no_face",
        bbox: null,
        yaw: null,
        face_count: 0,
      });
    });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 550));
    });

    expect(analyzeMock).toHaveBeenCalledTimes(2);
  });
});
