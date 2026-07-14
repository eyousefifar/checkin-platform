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
      setRows([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const filtered = useMemo(() => rows.filter((e) => matchesQuery(e, q)), [rows, q]);

  const showLoading = loading;
  const showError = !loading && Boolean(error);
  const showEmpty = !loading && !error && filtered.length === 0;
  const showRows = !loading && !error && filtered.length > 0;

  return (
    <div className="min-w-0 p-4 md:p-6">
      <div className="mb-6 flex min-w-0 flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          <h1 className="text-2xl font-bold uppercase tracking-wide text-ink">
            Employees
          </h1>
          <p className="mt-1 text-sm text-body">Enrollment gallery · embeddings</p>
        </div>
        <Link
          href="/employees/new"
          className="border border-ink px-6 py-3 text-sm font-bold uppercase tracking-label text-ink hover:bg-elevated focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
        >
          Add employee
        </Link>
      </div>

      <div className="mb-4">
        <label
          htmlFor="employee-search"
          className="block max-w-md text-sm font-bold uppercase tracking-label text-body"
        >
          Search employees
        </label>
        <input
          id="employee-search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Search name, code, or department…"
          className="mt-2 w-full max-w-md border border-hairline bg-card px-3 py-2 text-sm font-normal normal-case tracking-normal text-ink outline-none focus:border-cyan focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
          data-testid="employee-search"
        />
      </div>

      {showError && (
        <div
          className="mb-4 border border-danger/40 bg-danger/10 px-4 py-3 text-sm text-danger"
          role="alert"
          data-testid="employees-error"
        >
          {error} —{" "}
          <Link href="/login" className="underline">
            login
          </Link>{" "}
          if unauthorized
        </div>
      )}

      {showLoading && (
        <div
          className="border border-hairline px-4 py-8 text-sm text-body"
          role="status"
          data-testid="employees-loading"
        >
          Loading…
        </div>
      )}

      {showEmpty && (
        <div
          className="border border-hairline px-4 py-8 text-sm text-body"
          data-testid="employees-empty"
        >
          No employees yet.
        </div>
      )}

      {showRows && (
        <div className="min-w-0 overflow-x-auto border border-hairline">
          <table className="w-full min-w-[36rem] text-left text-sm">
            <thead className="border-b border-hairline bg-card text-xs uppercase tracking-label text-body">
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
              {filtered.map((e) => (
                <tr
                  key={e.id}
                  className="border-b border-hairline/60 hover:bg-card/80"
                  data-testid="employee-row"
                >
                  <td className="px-4 py-3 font-mono text-xs">
                    <Link
                      href={`/employees/${e.id}`}
                      className="text-ink underline-offset-2 hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
                    >
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
                        e.embedding_ready ? "text-signal" : "text-body"
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
      )}
    </div>
  );
}
