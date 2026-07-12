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

  const showDailyLoading = loading;
  const showDailyError = !loading && Boolean(error);
  const showDailyEmpty = !loading && !error && filtered.length === 0;
  const showDailyRows = !loading && !error && filtered.length > 0;

  return (
    <div className="min-w-0 p-4 md:p-6">
      <div className="mb-6 flex min-w-0 flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          <h1 className="text-2xl font-bold uppercase tracking-wide text-ink">
            Attendance
          </h1>
          <p className="mt-1 text-sm text-body">Daily sheet · CSV export</p>
        </div>
        <button
          type="button"
          onClick={() => void exportCsv()}
          disabled={!date}
          className="border border-ink px-6 py-3 text-sm font-bold uppercase tracking-label text-ink hover:bg-elevated focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark disabled:opacity-40"
        >
          Export CSV
        </button>
      </div>

      {waitingForHealth && (
        <div
          data-testid="attendance-health-loading"
          className="mb-4 border border-hairline bg-card px-4 py-6 text-sm text-body"
          role="status"
        >
          Loading attendance calendar timezone…
        </div>
      )}

      {healthError && !timezone && (
        <div
          className="mb-4 border border-m-red/40 bg-m-red/10 px-4 py-3 text-sm text-m-red"
          role="alert"
        >
          Health unavailable: {healthError}. Retrying…
        </div>
      )}

      {timezone && date && (
        <>
          <div className="mb-4 flex min-w-0 flex-wrap items-center gap-4">
            <label
              htmlFor="attendance-date"
              className="text-sm font-bold uppercase tracking-label text-body"
            >
              Date
            </label>
            <input
              id="attendance-date"
              type="date"
              value={date}
              onChange={(e) => setDate(e.target.value)}
              className="border border-hairline bg-card px-2 py-1 font-mono text-sm text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark"
            />
            <span
              data-testid="attendance-timezone"
              className="font-mono text-xs text-body"
            >
              {timezone}
            </span>
            <div className="flex flex-wrap gap-2" role="group" aria-label="Status filter">
              {chips.map((c) => (
                <button
                  key={c}
                  type="button"
                  onClick={() => setStatus(c)}
                  aria-pressed={status === c}
                  className={`border px-3 py-1 text-xs font-bold uppercase tracking-label focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark ${
                    status === c
                      ? "border-m-blue-dark text-ink"
                      : "border-hairline text-body hover:text-ink"
                  }`}
                >
                  {c}
                </button>
              ))}
            </div>
          </div>

          {showDailyError && (
            <div
              className="mb-4 border border-m-red/40 bg-m-red/10 px-4 py-3 text-sm text-m-red"
              role="alert"
              data-testid="attendance-error"
            >
              {error}
            </div>
          )}

          {eventsError && !showDailyError && (
            <div
              className="mb-4 border border-warning/40 bg-warning/10 px-4 py-3 text-sm text-warning"
              role="status"
              data-testid="events-load-error"
            >
              Raw events unavailable: {eventsError}. Daily aggregates remain
              usable.
            </div>
          )}

          {showDailyLoading && (
            <div
              className="border border-hairline px-4 py-8 text-sm text-body"
              role="status"
              data-testid="attendance-loading"
            >
              Loading…
            </div>
          )}

          {showDailyEmpty && (
            <div
              className="border border-hairline px-4 py-8 text-sm text-body"
              data-testid="attendance-empty"
            >
              No rows for this day.
            </div>
          )}

          {showDailyRows && (
          <div className="min-w-0 overflow-x-auto border border-hairline">
            <table className="w-full min-w-[40rem] text-left text-sm">
              <thead className="border-b border-hairline bg-card text-xs uppercase tracking-label text-body">
                <tr>
                  <th className="w-12 px-4 py-3">
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
                {filtered.map((r) => {
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
                              className="border border-hairline px-2 py-1 text-xs font-bold uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark"
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
          )}
        </>
      )}
    </div>
  );
}
