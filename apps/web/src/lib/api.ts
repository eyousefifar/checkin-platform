const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8000";

export function getToken(): string | null {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return null;
  return localStorage.getItem("pksp_token");
}

export function setToken(token: string | null) {
  if (typeof window === "undefined" || typeof localStorage === "undefined") return;
  if (token) localStorage.setItem("pksp_token", token);
  else localStorage.removeItem("pksp_token");
}

export async function api<T = unknown>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const token = getToken();
  const headers = new Headers(options.headers);
  if (!(options.body instanceof FormData) && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  if (token) headers.set("Authorization", `Bearer ${token}`);

  const res = await fetch(`${API_URL}${path}`, { ...options, headers });
  if (res.status === 401 && typeof window !== "undefined") {
    // soft redirect
    if (!path.includes("/auth/login")) {
      setToken(null);
    }
  }
  if (!res.ok) {
    let detail = res.statusText;
    try {
      const j = await res.json();
      detail = j.detail || JSON.stringify(j);
    } catch {
      /* ignore */
    }
    throw new Error(typeof detail === "string" ? detail : JSON.stringify(detail));
  }
  const ct = res.headers.get("content-type") || "";
  if (ct.includes("application/json")) return res.json();
  return res.text() as unknown as T;
}

export { API_URL };

/** Live WebSocket URL — never appends the browser JWT (Bearer remains for HTTP). */
export function wsUrl(): string {
  return process.env.NEXT_PUBLIC_WS_URL || "ws://localhost:8000/api/ws/live";
}

/** Absolute URL for a relative snapshot path from the API. */
export function snapshotAbsoluteUrl(snapshotUrl: string): string {
  if (snapshotUrl.startsWith("http://") || snapshotUrl.startsWith("https://")) {
    return snapshotUrl;
  }
  return `${API_URL}${snapshotUrl.startsWith("/") ? "" : "/"}${snapshotUrl}`;
}

/**
 * Analyze one enrollment candidate frame (multipart file).
 * Uses the same auth + body rules as enrollment upload.
 */
export async function analyzeEnrollmentFrame(
  file: Blob,
  filename = "frame.jpg",
  signal?: AbortSignal,
): Promise<import("./types").EnrollmentAnalyzeResult> {
  const fd = new FormData();
  fd.append("file", file, filename);
  return api("/api/enrollment/analyze", { method: "POST", body: fd, signal });
}
