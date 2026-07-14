"use client";

import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { API_URL } from "@/lib/api";
import type { HealthResponse } from "@/lib/types";

const INITIAL_RETRY_MS = 500;
const MAX_RETRY_MS = 10000;

export type UseHealthResult = {
  data: HealthResponse | null;
  loading: boolean;
  error: string | null;
  refresh: () => void;
};

const HealthContext = createContext<UseHealthResult | null>(null);

/**
 * Public `/api/health` owner. Mounted once by HealthProvider.
 */
function useHealthState(): UseHealthResult {
  const [data, setData] = useState<HealthResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const stoppedRef = useRef(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const retryRef = useRef(0);
  const hasDataRef = useRef(false);

  const clearTimer = () => {
    if (timerRef.current != null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const scheduleRetry = useCallback((run: () => void) => {
    clearTimer();
    const delay = Math.min(MAX_RETRY_MS, INITIAL_RETRY_MS * 2 ** retryRef.current);
    retryRef.current += 1;
    timerRef.current = setTimeout(run, delay);
  }, []);

  const fetchOnce = useCallback(() => {
    if (stoppedRef.current) return;
    clearTimer();
    if (abortRef.current) {
      abortRef.current.abort();
    }
    const ac = new AbortController();
    abortRef.current = ac;

    if (!hasDataRef.current) {
      setLoading(true);
    }

    void (async () => {
      try {
        const res = await fetch(`${API_URL}/api/health`, { signal: ac.signal });
        if (!res.ok) {
          throw new Error(`health ${res.status}`);
        }
        const body = (await res.json()) as HealthResponse;
        if (stoppedRef.current || ac.signal.aborted) return;
        if (typeof body.timezone !== "string" || !body.timezone) {
          throw new Error("health missing timezone");
        }
        hasDataRef.current = true;
        setData(body);
        setError(null);
        setLoading(false);
        retryRef.current = 0;
      } catch (e) {
        if (stoppedRef.current || ac.signal.aborted) return;
        const msg = e instanceof Error ? e.message : "health failed";
        setError(msg);
        if (!hasDataRef.current) {
          setLoading(true);
        }
        scheduleRetry(() => {
          if (!stoppedRef.current) fetchOnce();
        });
      }
    })();
  }, [scheduleRetry]);

  const refresh = useCallback(() => {
    retryRef.current = 0;
    fetchOnce();
  }, [fetchOnce]);

  useEffect(() => {
    stoppedRef.current = false;
    fetchOnce();
    return () => {
      stoppedRef.current = true;
      clearTimer();
      if (abortRef.current) {
        abortRef.current.abort();
        abortRef.current = null;
      }
    };
  }, [fetchOnce]);

  return { data, loading, error, refresh };
}

export function HealthProvider({ children }: { children: ReactNode }) {
  const value = useHealthState();
  return createElement(HealthContext.Provider, { value }, children);
}

export function useHealth(): UseHealthResult {
  const value = useContext(HealthContext);
  if (!value) throw new Error("useHealth must be used inside HealthProvider");
  return value;
}
