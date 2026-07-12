"use client";

import type { AttendanceMsg } from "@/lib/types";
import { timeFromEpochSeconds } from "@/lib/dateTime";

function kindColor(kind: string) {
  if (kind === "check_in") return "text-success";
  if (kind === "check_out") return "text-m-blue-dark";
  if (kind.includes("spoof")) return "text-m-red";
  return "text-muted";
}

export function EventTicker({
  events,
  timezone,
}: {
  events: AttendanceMsg[];
  /** Configured APP_TIMEZONE from health; omit until health succeeds. */
  timezone?: string | null;
}) {
  return (
    <div className="flex h-full flex-col border border-hairline bg-card">
      <div className="border-b border-hairline px-4 py-3 text-xs font-bold uppercase tracking-label text-body">
        Event ticker
      </div>
      <ul
        className="flex-1 space-y-2 overflow-y-auto p-3 font-mono text-xs"
        role="status"
        aria-live="polite"
        aria-relevant="additions"
        data-testid="event-ticker-list"
      >
        {events.length === 0 && (
          <li className="text-body">Waiting for live events…</li>
        )}
        {events.map((e, i) => (
          <li
            key={`${e.event_id}-${i}`}
            className="flex items-start justify-between gap-2 border-b border-hairline/50 pb-2"
            data-testid="event-ticker-item"
          >
            <div className="min-w-0">
              <span className="text-ink">{e.name}</span>
              <span className={`ml-2 uppercase ${kindColor(e.kind)}`}>
                {e.kind.replace("_", "-")}
              </span>
              <div className="mt-0.5 text-xs text-body">
                <span data-testid="event-ticker-time">
                  {timeFromEpochSeconds(e.ts, timezone)}
                </span>
                <span className="mx-1">·</span>
                <span data-testid="event-ticker-camera">{e.camera_id}</span>
              </div>
            </div>
            <span className="shrink-0 text-body">
              {e.score?.toFixed?.(2) ?? "—"}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
