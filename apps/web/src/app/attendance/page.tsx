"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { API_URL, api, getToken } from "@/lib/api";
import { StatusBadge } from "@/components/StatusBadge";
import type { DailyRow } from "@/lib/types";

function todayISO() {
  return new Date().toISOString().slice(0, 10);
}

export default function AttendancePage() {
  const [date, setDate] = useState(todayISO);
  const [status, setStatus] = useState("all");
  const [rows, setRows] = useState<DailyRow[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const data = await api<DailyRow[]>(`/api/attendance/daily?date=${date}`);
      setRows(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed");
    } finally {
      setLoading(false);
    }
  }, [date]);

  useEffect(() => {
    load();
  }, [load]);

  const filtered = useMemo(() => {
    if (status === "all") return rows;
    return rows.filter((r) => r.status === status);
  }, [rows, status]);

  async function exportCsv() {
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
          onClick={exportCsv}
          className="border border-ink px-6 py-3 text-xs font-bold uppercase tracking-label text-ink hover:bg-elevated"
        >
          Export CSV
        </button>
      </div>

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
        <div className="mb-4 border border-m-red/40 bg-m-red/10 px-4 py-3 text-sm text-m-red">
          {error}
        </div>
      )}

      <div className="overflow-x-auto border border-hairline">
        <table className="w-full text-left text-sm">
          <thead className="border-b border-hairline bg-card text-[10px] uppercase tracking-label text-muted">
            <tr>
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
                <td colSpan={7} className="px-4 py-8 text-muted">
                  Loading…
                </td>
              </tr>
            )}
            {!loading && filtered.length === 0 && (
              <tr>
                <td colSpan={7} className="px-4 py-8 text-muted">
                  No rows for this day.
                </td>
              </tr>
            )}
            {filtered.map((r) => (
              <tr key={r.employee_id} className="border-b border-hairline/60">
                <td className="px-4 py-3 font-mono text-xs">{r.employee_code}</td>
                <td className="px-4 py-3 text-ink">{r.full_name}</td>
                <td className="px-4 py-3 font-mono text-xs text-body">
                  {r.first_in ? r.first_in.slice(11, 19) : "—"}
                </td>
                <td className="px-4 py-3 font-mono text-xs text-body">
                  {r.last_out ? r.last_out.slice(11, 19) : "—"}
                </td>
                <td className="px-4 py-3 font-mono text-xs">
                  {r.duration_minutes != null ? `${r.duration_minutes}m` : "—"}
                </td>
                <td className="px-4 py-3">
                  <StatusBadge status={r.status} />
                </td>
                <td className="px-4 py-3 font-mono text-xs text-muted">
                  {r.check_in_count}/{r.check_out_count}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
