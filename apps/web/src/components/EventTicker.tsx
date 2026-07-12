"use client";

import type { AttendanceMsg } from "@/lib/types";

function kindColor(kind: string) {
  if (kind === "check_in") return "text-success";
  if (kind === "check_out") return "text-m-blue-dark";
  if (kind.includes("spoof")) return "text-m-red";
  return "text-muted";
}

export function EventTicker({ events }: { events: AttendanceMsg[] }) {
  return (
    <div className="flex h-full flex-col border border-hairline bg-card">
      <div className="border-b border-hairline px-4 py-3 text-[11px] font-bold uppercase tracking-label text-muted">
        Event ticker
      </div>
      <ul className="flex-1 space-y-2 overflow-y-auto p-3 font-mono text-xs">
        {events.length === 0 && (
          <li className="text-muted">Waiting for live events…</li>
        )}
        {events.map((e, i) => (
          <li
            key={`${e.event_id}-${i}`}
            className="flex items-start justify-between gap-2 border-b border-hairline/50 pb-2"
          >
            <div>
              <span className="text-ink">{e.name}</span>
              <span className={`ml-2 uppercase ${kindColor(e.kind)}`}>
                {e.kind.replace("_", "-")}
              </span>
            </div>
            <span className="shrink-0 text-muted">
              {e.score?.toFixed?.(2) ?? "—"}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
