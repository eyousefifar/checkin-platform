"use client";

import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { API_URL, api, getToken } from "@/lib/api";
import { StatusBadge } from "@/components/StatusBadge";
import { useHealth } from "@/hooks/useHealth";
import { calendarDateInZone, timeInZone } from "@/lib/dateTime";
import type { DailyRow, RawAttendanceEvent } from "@/lib/types";

export default function AttendancePage() {
  const { data: health, loading: healthLoading, error: healthError } = useHealth();
  const timezone = health?.timezone ?? null;

  // Date is initialized exactly once from health timezone; later refreshes must
  // not overwrite a user-selected day.
  const [date, setDate] = useState<string | null>(null);
  const dateInitializedRef = useRef(false);
  const [status, setStatus] = useState("all");
  const [rows, setRows] = useState<DailyRow[]>([]);
  const [events, setEvents] = useState<RawAttendanceEvent[]>([]);
  const [error, setError] = useState("");
  const [eventsError, setEventsError] = useState("");
  const [loading, setLoading] = useState(false);
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set());
  const requestGenRef = useRef(0);

  useEffect(() => {
    if (!timezone || dateInitializedRef.current) return;
    dateInitializedRef.current = true;
    setDate(calendarDateInZone(new Date(), timezone));
  }, [timezone]);

  const load = useCallback(async () => {
    if (!date) return;
    const gen = ++requestGenRef.current;
    setLoading(true);
    setError("");
    setEventsError("");
    // Collapse expansions when the selected day changes.
    setExpanded(new Set());

    const dailyP = api<DailyRow[]>(`/api/attendance/daily?date=${date}`);
    const eventsP = api<RawAttendanceEvent[]>(
      `/api/attendance/events?date=${date}`,
    );

    const [dailyResult, eventsResult] = await Promise.allSettled([
      dailyP,
      eventsP,
    ]);

    if (gen !== requestGenRef.current) {
      // Stale date response — do not overwrite current state.
      return;
    }

    if (dailyResult.status === "fulfilled") {
      setRows(dailyResult.value);
    } else {
      const reason = dailyResult.reason;
      setError(reason instanceof Error ? reason.message : "Failed");
      setRows([]);
    }

    if (eventsResult.status === "fulfilled") {
      setEvents(eventsResult.value);
      setEventsError("");
    } else {
      const reason = eventsResult.reason;
      setEvents([]);
      setEventsError(
        reason instanceof Error ? reason.message : "Failed to load events",
      );
    }

    setLoading(false);
  }, [date]);

  useEffect(() => {
    void load();
  }, [load]);

  const filtered = useMemo(() => {
    if (status === "all") return rows;
    return rows.filter((r) => r.status === status);
  }, [rows, status]);

  const eventsByEmployee = useMemo(() => {
    const map = new Map<number, RawAttendanceEvent[]>();
    for (const ev of events) {
      if (ev.employee_id == null) continue;
      const list = map.get(ev.employee_id);
      if (list) list.push(ev);
      else map.set(ev.employee_id, [ev]);
    }
    // API returns newest-first; detail rows show oldest-to-newest.
    for (const list of map.values()) {
      list.sort((a, b) => {
        if (a.ts < b.ts) return -1;
        if (a.ts > b.ts) return 1;
        return a.id - b.id;
      });
    }
    return map;
  }, [events]);

  function toggleExpand(employeeId: number) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(employeeId)) next.delete(employeeId);
      else next.add(employeeId);
      return next;
    });
  }

  async function exportCsv() {
    if (!date) return;
    const token = getToken();
    const res = await fetch(
      `${API_URL}/api/attendance/daily.csv?date=${date}`,
      { headers: token ? { Authorization: `Bearer ${token}` } : {} },
    );
    if (!res.ok) {
      setError("CSV export failed — login required?");
      return;
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `attendance-${date}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }

  const chips = ["all", "present", "incomplete", "absent", "anomaly"];
  const waitingForHealth = healthLoading && !timezone;
  const zone = timezone ?? "UTC";

  return (
    <div className="p-6">
      <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold uppercase tracking-wide text-ink">
            Attendance
          </h1>
          <p className="mt-1 text-sm text-body">Daily sheet · CSV export</p>
        </div>
        <button
          type="button"
          onClick={() => void exportCsv()}
          disabled={!date}
          className="border border-ink px-6 py-3 text-xs font-bold uppercase tracking-label text-ink hover:bg-elevated disabled:opacity-40"
        >
          Export CSV
        </button>
      </div>

      {waitingForHealth && (
        <div
          data-testid="attendance-health-loading"
          className="mb-4 border border-hairline bg-card px-4 py-6 text-sm text-muted"
        >
          Loading attendance calendar timezone…
        </div>
      )}

      {healthError && !timezone && (
        <div className="mb-4 border border-m-red/40 bg-m-red/10 px-4 py-3 text-sm text-m-red">
          Health unavailable: {healthError}. Retrying…
        </div>
      )}

      {timezone && date && (
        <>
          <div className="mb-4 flex flex-wrap items-center gap-4">
            <label className="text-[11px] font-bold uppercase tracking-label text-muted">
              Date
              <input
                type="date"
                value={date}
                onChange={(e) => setDate(e.target.value)}
                className="ml-2 border border-hairline bg-card px-2 py-1 font-mono text-sm text-ink"
              />
            </label>
            <span
              data-testid="attendance-timezone"
              className="font-mono text-xs text-muted"
            >
              {timezone}
            </span>
            <div className="flex flex-wrap gap-2">
              {chips.map((c) => (
                <button
                  key={c}
                  type="button"
                  onClick={() => setStatus(c)}
                  className={`border px-3 py-1 text-[10px] font-bold uppercase tracking-label ${
                    status === c
                      ? "border-m-blue-dark text-ink"
                      : "border-hairline text-muted hover:text-ink"
                  }`}
                >
                  {c}
                </button>
              ))}
            </div>
          </div>

          {error && (
            <div
              className="mb-4 border border-m-red/40 bg-m-red/10 px-4 py-3 text-sm text-m-red"
              role="alert"
            >
              {error}
            </div>
          )}

          {eventsError && (
            <div
              className="mb-4 border border-warning/40 bg-warning/10 px-4 py-3 text-sm text-warning"
              role="status"
              data-testid="events-load-error"
            >
              Raw events unavailable: {eventsError}. Daily aggregates remain
              usable.
            </div>
          )}

          <div className="overflow-x-auto border border-hairline">
            <table className="w-full text-left text-sm">
              <thead className="border-b border-hairline bg-card text-[10px] uppercase tracking-label text-muted">
                <tr>
                  <th className="px-4 py-3 w-12">
                    <span className="sr-only">Details</span>
                  </th>
                  <th className="px-4 py-3">Code</th>
                  <th className="px-4 py-3">Name</th>
                  <th className="px-4 py-3">First in</th>
                  <th className="px-4 py-3">Last out</th>
                  <th className="px-4 py-3">Duration</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">In/Out</th>
                </tr>
              </thead>
              <tbody>
                {loading && (
                  <tr>
                    <td colSpan={8} className="px-4 py-8 text-muted">
                      Loading…
                    </td>
                  </tr>
                )}
                {!loading && filtered.length === 0 && (
                  <tr>
                    <td colSpan={8} className="px-4 py-8 text-muted">
                      No rows for this day.
                    </td>
                  </tr>
                )}
                {!loading &&
                  filtered.map((r) => {
                    const isOpen = expanded.has(r.employee_id);
                    const detailId = `attendance-detail-${r.employee_id}`;
                    const rowEvents = eventsByEmployee.get(r.employee_id) ?? [];
                    return (
                      <Fragment key={r.employee_id}>
                        <tr className="border-b border-hairline/60">

                          <td className="px-4 py-3">
                            <button
                              type="button"
                              aria-expanded={isOpen}
                              aria-controls={detailId}
                              onClick={() => toggleExpand(r.employee_id)}
                              className="border border-hairline px-2 py-1 text-[10px] font-bold uppercase tracking-label text-body hover:text-ink"
                              data-testid={`expand-${r.employee_id}`}
                            >
                              {isOpen ? "Hide" : "Events"}
                            </button>
                          </td>
                          <td className="px-4 py-3 font-mono text-xs">
                            {r.employee_code}
                          </td>
                          <td className="px-4 py-3 text-ink">{r.full_name}</td>
                          <td
                            className="px-4 py-3 font-mono text-xs text-body"
                            data-testid={`first-in-${r.employee_id}`}
                          >
                            {timeInZone(r.first_in, zone)}
                          </td>
                          <td
                            className="px-4 py-3 font-mono text-xs text-body"
                            data-testid={`last-out-${r.employee_id}`}
                          >
                            {timeInZone(r.last_out, zone)}
                          </td>
                          <td className="px-4 py-3 font-mono text-xs">
                            {r.duration_minutes != null
                              ? `${r.duration_minutes}m`
                              : "—"}
                          </td>
                          <td className="px-4 py-3">
                            <StatusBadge status={r.status} />
                          </td>
                          <td className="px-4 py-3 font-mono text-xs text-muted">
                            {r.check_in_count}/{r.check_out_count}
                          </td>
                        </tr>
                        {isOpen && (
                          <tr className="border-b border-hairline/60 bg-soft">
                            <td colSpan={8} className="px-4 py-3">
                              <div
                                id={detailId}
                                role="region"
                                aria-label={`Events for ${r.full_name}`}
                                data-testid={`detail-${r.employee_id}`}
                              >
                                {eventsError ? (
                                  <p
                                    className="text-sm text-warning"
                                    data-testid={`detail-error-${r.employee_id}`}
                                  >
                                    Could not load events for this day:{" "}
                                    {eventsError}
                                  </p>
                                ) : rowEvents.length === 0 ? (
                                  <p
                                    className="text-sm text-muted"
                                    data-testid={`detail-empty-${r.employee_id}`}
                                  >
                                    No events for this employee on this day.
                                  </p>
                                ) : (
                                  <ul
                                    className="space-y-1 font-mono text-xs text-body"
                                    data-testid={`detail-events-${r.employee_id}`}
                                  >
                                    {rowEvents.map((ev) => (
                                      <li
                                        key={ev.id}
                                        data-testid={`raw-event-${ev.id}`}
                                        className="flex flex-wrap gap-x-3 gap-y-1"
                                      >
                                        <span data-field="time">
                                          {timeInZone(ev.ts, zone)}
                                        </span>
                                        <span
                                          data-field="kind"
                                          className="uppercase"
                                        >
                                          {ev.kind.replace(/_/g, "-")}
                                        </span>
                                        <span data-field="camera">
                                          {ev.camera_id}
                                        </span>
                                        <span data-field="score">
                                          {ev.score != null
                                            ? ev.score.toFixed(2)
                                            : "—"}
                                        </span>
                                      </li>
                                    ))}
                                  </ul>
                                )}
                              </div>
                            </td>
                          </tr>
                        )}
                      </Fragment>
                    );
                  })}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}
