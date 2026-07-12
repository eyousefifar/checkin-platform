import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useHealth } from "./useHealth";

const okBody = {
  status: "ok",
  timezone: "Asia/Tehran",
  vision_ready: true,
  vision_provider: "mock",
  gallery_size: 0,
  cameras: [],
  media: {
    mediamtx_running: false,
    transcoder_running: false,
    publication: "unavailable",
    source_mode: null,
    preferred_webrtc_path: null,
    last_error: null,
  },
};

describe("useHealth", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("recovers after an initial failure and replaces data atomically", async () => {
    const fetchMock = vi
      .fn()
      .mockRejectedValueOnce(new Error("network down"))
      .mockResolvedValueOnce({
        ok: true,
        json: async () => okBody,
      });
    vi.stubGlobal("fetch", fetchMock);

    const { result } = renderHook(() => useHealth());
    expect(result.current.loading).toBe(true);
    expect(result.current.data).toBeNull();

    // Flush microtasks for the first rejected fetch.
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(result.current.error).toBe("network down");
    expect(result.current.data).toBeNull();
    expect(result.current.loading).toBe(true);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    // Advance capped retry timer; second fetch succeeds.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(result.current.data?.timezone).toBe("Asia/Tehran");
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it("stops timers and in-flight work on unmount", async () => {
    let resolveFetch: ((v: unknown) => void) | null = null;
    const fetchMock = vi.fn().mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveFetch = resolve;
        }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const { unmount } = renderHook(() => useHealth());
    expect(fetchMock).toHaveBeenCalledTimes(1);

    unmount();

    // Resolving after unmount must not throw or schedule further work.
    await act(async () => {
      resolveFetch?.({
        ok: true,
        json: async () => ({ ...okBody, timezone: "UTC" }),
      });
      await Promise.resolve();
      await vi.advanceTimersByTimeAsync(20000);
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
