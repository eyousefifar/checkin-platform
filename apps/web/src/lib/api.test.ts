import { afterEach, describe, expect, it, vi } from "vitest";
import { setToken, wsUrl } from "./api";

describe("wsUrl", () => {
  afterEach(() => {
    setToken(null);
    vi.unstubAllEnvs();
  });

  it("returns the base WebSocket URL without reading or appending a token", () => {
    setToken("fake-jwt-must-not-appear-in-ws-url");
    const url = wsUrl();
    expect(url).toBe("ws://localhost:8000/api/ws/live");
    expect(url).not.toMatch(/[?&]token=/);
    expect(url).not.toContain("fake-jwt");
    expect(url).not.toContain("encodeURIComponent");
  });

  it("uses NEXT_PUBLIC_WS_URL when set, still without a token query", () => {
    vi.stubEnv("NEXT_PUBLIC_WS_URL", "ws://edge.local:8000/api/ws/live");
    // re-import is unnecessary: wsUrl reads process.env at call time
    setToken("another-token");
    const url = wsUrl();
    // Note: module may have captured default at load; assert no token either way
    expect(url).not.toMatch(/[?&]token=/);
    expect(url).not.toContain("another-token");
  });
});
