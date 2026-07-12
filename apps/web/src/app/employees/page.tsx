"use client";

import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import type { Employee } from "@/lib/types";

function matchesQuery(e: Employee, q: string): boolean {
  const needle = q.trim().toLowerCase();
  if (!needle) return true;
  const code = e.employee_code.toLowerCase();
  const name = e.full_name.toLowerCase();
  const dept = (e.department || "").toLowerCase();
  return code.includes(needle) || name.includes(needle) || dept.includes(needle);
}

export default function EmployeesPage() {
  const [rows, setRows] = useState<Employee[]>([]);
  const [q, setQ] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      // Fetch once; filter locally on keystrokes (sub-50 scale).
      const data = await api<Employee[]>("/api/employees");
      setRows(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const filtered = useMemo(() => rows.filter((e) => matchesQuery(e, q)), [rows, q]);

  return (
    <div className="p-6">
      <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold uppercase tracking-wide text-ink">
            Employees
          </h1>
          <p className="mt-1 text-sm text-body">Enrollment gallery · embeddings</p>
        </div>
        <Link
          href="/employees/new"
          className="border border-ink px-6 py-3 text-xs font-bold uppercase tracking-label text-ink hover:bg-elevated"
        >
          Add employee
        </Link>
      </div>

      <div className="mb-4">
        <label className="block max-w-md text-[11px] font-bold uppercase tracking-label text-muted">
          Search employees
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Search name, code, or department…"
            className="mt-2 w-full border border-hairline bg-card px-3 py-2 text-sm font-normal normal-case tracking-normal text-ink outline-none focus:border-m-blue-dark"
            data-testid="employee-search"
          />
        </label>
      </div>

      {error && (
        <div className="mb-4 border border-m-red/40 bg-m-red/10 px-4 py-3 text-sm text-m-red">
          {error} — <Link href="/login" className="underline">login</Link> if unauthorized
        </div>
      )}

      <div className="overflow-x-auto border border-hairline">
        <table className="w-full text-left text-sm">
          <thead className="border-b border-hairline bg-card text-[10px] uppercase tracking-label text-muted">
            <tr>
              <th className="px-4 py-3">Code</th>
              <th className="px-4 py-3">Name</th>
              <th className="px-4 py-3">Dept</th>
              <th className="px-4 py-3">Usable imgs</th>
              <th className="px-4 py-3">Embedding</th>
              <th className="px-4 py-3">Active</th>
            </tr>
          </thead>
          <tbody>
            {loading && (
              <tr>
                <td colSpan={6} className="px-4 py-8 text-muted">
                  Loading…
                </td>
              </tr>
            )}
            {!loading && filtered.length === 0 && (
              <tr>
                <td colSpan={6} className="px-4 py-8 text-muted">
                  No employees yet.
                </td>
              </tr>
            )}
            {!loading &&
              filtered.map((e) => (
                <tr
                  key={e.id}
                  className="border-b border-hairline/60 hover:bg-card/80"
                  data-testid="employee-row"
                >
                  <td className="px-4 py-3 font-mono text-xs">
                    <Link href={`/employees/${e.id}`} className="text-m-blue-light hover:underline">
                      {e.employee_code}
                    </Link>
                  </td>
                  <td className="px-4 py-3 text-ink">{e.full_name}</td>
                  <td className="px-4 py-3 text-body">{e.department || "—"}</td>
                  <td className="px-4 py-3 font-mono text-xs">
                    {e.usable_images}/{e.image_count}
                  </td>
                  <td className="px-4 py-3">
                    <span
                      className={
                        e.embedding_ready ? "text-success" : "text-muted"
                      }
                    >
                      {e.embedding_ready ? "ready" : "missing"}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-xs uppercase tracking-label">
                    {e.is_active ? "yes" : "no"}
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
