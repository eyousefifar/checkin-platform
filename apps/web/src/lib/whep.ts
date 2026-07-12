/**
 * MediaMTX WHEP reader — native WebRTC, no extra deps.
 * Endpoint pattern: {base}/{path}/whep  (POST application/sdp)
 */

export function whepUrl(base: string, path: string): string {
  const b = base.replace(/\/$/, "");
  const p = path.replace(/^\//, "");
  return `${b}/${p}/whep`;
}

function waitIceGathering(pc: RTCPeerConnection, timeoutMs = 2000): Promise<void> {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const t = setTimeout(() => resolve(), timeoutMs);
    pc.onicegatheringstatechange = () => {
      if (pc.iceGatheringState === "complete") {
        clearTimeout(t);
        resolve();
      }
    };
  });
}

export type WhepHandle = {
  pc: RTCPeerConnection;
  close: () => void;
};

export async function connectWhep(
  endpoint: string,
  video: HTMLVideoElement,
): Promise<WhepHandle> {
  const pc = new RTCPeerConnection({
    iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
  });

  pc.addTransceiver("video", { direction: "recvonly" });
  pc.addTransceiver("audio", { direction: "recvonly" });

  pc.ontrack = (ev) => {
    if (ev.streams[0]) {
      video.srcObject = ev.streams[0];
      void video.play().catch(() => {
        /* autoplay policies */
      });
    }
  };

  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);
  await waitIceGathering(pc);

  const res = await fetch(endpoint, {
    method: "POST",
    headers: {
      "Content-Type": "application/sdp",
    },
    body: pc.localDescription?.sdp ?? offer.sdp,
  });

  if (!res.ok) {
    pc.close();
    throw new Error(`WHEP ${res.status} ${res.statusText}`);
  }

  const answerSdp = await res.text();
  await pc.setRemoteDescription({ type: "answer", sdp: answerSdp });

  return {
    pc,
    close: () => {
      try {
        pc.close();
      } catch {
        /* ignore */
      }
      if (video.srcObject) {
        const stream = video.srcObject as MediaStream;
        stream.getTracks().forEach((t) => t.stop());
        video.srcObject = null;
      }
    },
  };
}
