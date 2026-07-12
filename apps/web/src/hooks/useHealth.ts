"use client";

import { useCallback, useEffect, useRef, useState } from "react";
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

/**
 * Shared public `/api/health` fetch for timezone, cameras, and media status.
 * Native fetch only — no SWR/React Query/context. Retries with a capped
 * interval; replaces data atomically on success; cancels on unmount.
 */
export function useHealth(): UseHealthResult {
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
