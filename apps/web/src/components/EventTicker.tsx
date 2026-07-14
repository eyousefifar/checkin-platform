"use client";

import { useState } from "react";
import type { AttendanceMsg } from "@/lib/types";
import { timeFromEpochSeconds } from "@/lib/dateTime";
import { EventMatchReveal } from "./EventMatchReveal";

function kindColor(kind: string) {
  if (kind === "check_in") return "text-signal";
  if (kind === "check_out") return "text-cyan";
  if (kind.includes("spoof")) return "text-danger";
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
  const [inspect, setInspect] = useState<AttendanceMsg | null>(null);

  return (
    <div className="flex h-full flex-col border border-hairline bg-card">
      <div className="border-b border-hairline px-4 py-3 text-xs font-bold uppercase tracking-label text-body">
        Mission log
      </div>
      <ul
        className="flex-1 space-y-1 overflow-y-auto p-2 font-mono text-xs"
        role="status"
        aria-live="polite"
        aria-relevant="additions"
        data-testid="event-ticker-list"
      >
        {events.length === 0 && (
          <li className="px-2 py-2 text-body">Waiting for live events…</li>
        )}
        {events.map((e) => (
          <li key={e.event_id} className="list-none">
            <button
              type="button"
              onClick={() => setInspect(e)}
              className="flex w-full min-h-11 items-start justify-between gap-2 border border-transparent px-2 py-2 text-left hover:border-hairline hover:bg-elevated/40 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
              data-testid="event-ticker-item"
              aria-label={`Inspect event ${e.event_id}: ${e.name} ${e.kind}`}
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
                  {e.snapshot_url ? (
                    <span className="ml-2 text-cyan" data-testid="event-has-snap">
                      SNAP
                    </span>
                  ) : null}
                </div>
              </div>
              <span className="shrink-0 text-body">
                {e.score?.toFixed?.(2) ?? "—"}
              </span>
            </button>
          </li>
        ))}
      </ul>
      <EventMatchReveal
        event={inspect}
        timezone={timezone}
        onClose={() => setInspect(null)}
      />
    </div>
  );
}
