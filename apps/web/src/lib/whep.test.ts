import { afterEach, describe, expect, it, vi } from "vitest";
import { connectWhep, whepUrl } from "./whep";

describe("whepUrl", () => {
  it("trims one trailing base slash, one leading path slash, and builds /<path>/whep", () => {
    expect(whepUrl("http://localhost:8889/", "/demo")).toBe(
      "http://localhost:8889/demo/whep",
    );
    expect(whepUrl("http://localhost:8889", "demo")).toBe(
      "http://localhost:8889/demo/whep",
    );
  });
});

describe("connectWhep", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("uses on-prem ICE, video only, and owns the returned stream", async () => {
    const transceivers: string[] = [];
    const constructorArgs: unknown[][] = [];
    class PC extends EventTarget {
      iceGatheringState = "complete";
      localDescription = { sdp: "offer" };
      connectionState = "connected";
      ontrack: ((event: RTCTrackEvent) => void) | null = null;
      onconnectionstatechange = null;
      close = vi.fn();
      constructor(...args: unknown[]) {
        super();
        constructorArgs.push(args);
      }
      addTransceiver(kind: string) {
        transceivers.push(kind);
      }
      async createOffer() {
        return { type: "offer", sdp: "offer" };
      }
      async setLocalDescription() {}
      async setRemoteDescription() {}
    }
    class Stream extends EventTarget {
      tracks: MediaStreamTrack[] = [];
      getTracks() {
        return this.tracks;
      }
      addTrack(track: MediaStreamTrack) {
        this.tracks.push(track);
      }
    }
    vi.stubGlobal("RTCPeerConnection", PC);
    vi.stubGlobal("MediaStream", Stream);
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      text: async () => "answer",
    });
    vi.stubGlobal("fetch", fetchMock);

    const handle = await connectWhep("http://localhost:8889/demo/whep");
    expect(constructorArgs).toEqual([[]]);
    expect(transceivers).toEqual(["video"]);
    expect(fetchMock.mock.calls[0][1].signal).toBeInstanceOf(AbortSignal);

    const track = { id: "v1", stop: vi.fn() } as unknown as MediaStreamTrack;
    handle.pc.ontrack?.({ track } as RTCTrackEvent);
    handle.close();
    expect(track.stop).toHaveBeenCalledTimes(1);
    expect(handle.pc.close).toHaveBeenCalledTimes(1);
  });
});
