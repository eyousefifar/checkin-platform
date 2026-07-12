"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { FormEvent, useCallback, useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { Employee } from "@/lib/types";

export default function EmployeeDetailPage() {
  const params = useParams();
  const id = Number(params.id);
  const [emp, setEmp] = useState<Employee | null>(null);
  const [error, setError] = useState("");
  const [msg, setMsg] = useState("");
  const [files, setFiles] = useState<FileList | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const data = await api<Employee>(`/api/employees/${id}`);
      setEmp(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed");
    }
  }, [id]);

  useEffect(() => {
    load();
  }, [load]);

  async function upload(e: FormEvent) {
    e.preventDefault();
    if (!files?.length) return;
    setBusy(true);
    setMsg("");
    try {
      const fd = new FormData();
      Array.from(files).forEach((f) => fd.append("files", f));
      const up = await api<{
        usable: number;
        embedding_ready: boolean;
        rejected: { reason: string }[];
      }>(`/api/employees/${id}/images`, { method: "POST", body: fd });
      setMsg(
        `Upload: usable ${up.usable}, embedding ${up.embedding_ready ? "ready" : "pending"}. Rejects: ${up.rejected.map((r) => r.reason).join(", ") || "none"}`,
      );
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Upload failed");
    } finally {
      setBusy(false);
    }
  }

  async function recompute() {
    setBusy(true);
    try {
      const r = await api<{ embedding_ready: boolean; usable: number }>(
        `/api/employees/${id}/recompute-embedding`,
        { method: "POST" },
      );
      setMsg(`Recompute: usable ${r.usable}, ready=${r.embedding_ready}`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Recompute failed");
    } finally {
      setBusy(false);
    }
  }

  if (!emp && !error) {
    return <div className="p-6 text-muted">Loading…</div>;
  }

  return (
    <div className="mx-auto max-w-2xl p-6">
      <Link href="/employees" className="text-xs uppercase tracking-label text-muted hover:text-ink">
        ← Employees
      </Link>
      {error && <p className="mt-4 text-m-red">{error}</p>}
      {emp && (
        <>
          <h1 className="mt-4 text-2xl font-bold uppercase tracking-wide text-ink">
            {emp.full_name}
          </h1>
          <p className="mt-1 font-mono text-sm text-body">
            {emp.employee_code} · {emp.department || "no dept"} · embedding{" "}
            <span className={emp.embedding_ready ? "text-success" : "text-warning"}>
              {emp.embedding_ready ? "ready" : "missing"}
            </span>
          </p>

          <div className="mt-6 border border-hairline bg-card p-4">
            <h2 className="text-[11px] font-bold uppercase tracking-label text-muted">
              Images ({emp.image_count})
            </h2>
            <ul className="mt-2 space-y-1 font-mono text-xs text-body">
              {(emp.images || []).map((img) => (
                <li key={img.id}>
                  #{img.id} {img.usable ? "usable" : `reject:${img.reject_reason}`}
                </li>
              ))}
              {emp.image_count === 0 && <li className="text-muted">None yet</li>}
            </ul>
          </div>

          <form onSubmit={upload} className="mt-6 space-y-3">
            <input
              type="file"
              accept="image/*"
              multiple
              onChange={(e) => setFiles(e.target.files)}
              className="text-sm text-body"
            />
            <div className="flex gap-3">
              <button
                type="submit"
                disabled={busy}
                className="border border-ink px-4 py-2 text-xs font-bold uppercase tracking-label hover:bg-elevated disabled:opacity-50"
              >
                Upload images
              </button>
              <button
                type="button"
                onClick={recompute}
                disabled={busy}
                className="border border-hairline px-4 py-2 text-xs font-bold uppercase tracking-label text-body hover:text-ink disabled:opacity-50"
              >
                Recompute embedding
              </button>
            </div>
          </form>
          {msg && <p className="mt-3 text-sm text-success">{msg}</p>}
        </>
      )}
    </div>
  );
}
