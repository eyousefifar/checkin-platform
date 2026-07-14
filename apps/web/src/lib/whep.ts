/**
 * MediaMTX WHEP reader — native WebRTC, no extra deps.
 * Endpoint pattern: {base}/{path}/whep  (POST application/sdp)
 */

export function whepUrl(base: string, path: string): string {
  const b = base.replace(/\/$/, "");
  const p = path.replace(/^\//, "");
  return `${b}/${p}/whep`;
}

function waitIceGathering(pc: RTCPeerConnection, timeoutMs = 750): Promise<void> {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const done = () => {
      clearTimeout(timer);
      pc.removeEventListener("icegatheringstatechange", onChange);
      resolve();
    };
    const onChange = () => {
      if (pc.iceGatheringState === "complete") {
        done();
      }
    };
    const timer = setTimeout(done, timeoutMs);
    pc.addEventListener("icegatheringstatechange", onChange);
  });
}

export type WhepHandle = {
  pc: RTCPeerConnection;
  stream: MediaStream;
  close: () => void;
};

export async function connectWhep(endpoint: string): Promise<WhepHandle> {
  const pc = new RTCPeerConnection();
  const stream = new MediaStream();
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    pc.close();
    stream.getTracks().forEach((track) => track.stop());
  };

  pc.addTransceiver("video", { direction: "recvonly" });

  pc.ontrack = (ev) => {
    if (!stream.getTracks().some((track) => track.id === ev.track.id)) {
      stream.addTrack(ev.track);
    }
  };

  try {
    const offer = await pc.createOffer();
    await pc.setLocalDescription(offer);
    await waitIceGathering(pc);

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 3000);
    const res = await fetch(endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/sdp",
      },
      body: pc.localDescription?.sdp ?? offer.sdp,
      signal: controller.signal,
    }).finally(() => clearTimeout(timeout));

    if (!res.ok) {
      throw new Error(`WHEP ${res.status} ${res.statusText}`);
    }

    const answerSdp = await res.text();
    await pc.setRemoteDescription({ type: "answer", sdp: answerSdp });

    return { pc, stream, close };
  } catch (error) {
    close();
    throw error;
  }
}
