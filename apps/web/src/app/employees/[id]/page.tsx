"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import type { Employee, EnrollmentResult } from "@/lib/types";

export default function EmployeeDetailPage() {
  const params = useParams();
  const id = Number(params.id);
  const [emp, setEmp] = useState<Employee | null>(null);
  const [error, setError] = useState("");
  const [statusMsg, setStatusMsg] = useState("");
  const [uploadResult, setUploadResult] = useState<EnrollmentResult | null>(null);
  const [recomputeResult, setRecomputeResult] = useState<EnrollmentResult | null>(
    null,
  );
  const [files, setFiles] = useState<FileList | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const data = await api<Employee>(`/api/employees/${id}`);
      setEmp(data);
      setError("");
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
    setError("");
    setStatusMsg("");
    setUploadResult(null);
    setRecomputeResult(null);
    try {
      const fd = new FormData();
      Array.from(files).forEach((f) => fd.append("files", f));
      const up = await api<EnrollmentResult>(`/api/employees/${id}/images`, {
        method: "POST",
        body: fd,
      });
      setUploadResult(up);
      if (up.gallery_reload_pending) {
        setStatusMsg(
          "Images committed; recognition is catching up (gallery reload pending).",
        );
      } else {
        setStatusMsg(
          `Upload complete: usable ${up.usable}/${up.received}, embedding ${
            up.embedding_ready ? "ready" : "not ready"
          }.`,
        );
      }
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Upload failed");
      // Keep current employee; do not clear emp on upload failure.
    } finally {
      setBusy(false);
    }
  }

  async function recompute() {
    setBusy(true);
    setError("");
    setStatusMsg("");
    setRecomputeResult(null);
    try {
      const r = await api<EnrollmentResult>(
        `/api/employees/${id}/recompute-embedding`,
        { method: "POST" },
      );
      setRecomputeResult(r);
      setStatusMsg(
        `Recompute (no new files): usable ${r.usable}, embedding ${
          r.embedding_ready ? "ready" : "not ready"
        }, images used ${r.num_images_used}.`,
      );
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Recompute failed");
    } finally {
      setBusy(false);
    }
  }

  const resultRows = useMemo(() => {
    if (!uploadResult) return [];
    if (uploadResult.results && uploadResult.results.length > 0) {
      return uploadResult.results;
    }
    return uploadResult.rejected.map((r) => ({
      filename: r.filename,
      usable: false,
      reason: r.reason,
    }));
  }, [uploadResult]);

  if (!emp && !error) {
    return <div className="p-6 text-muted">Loading…</div>;
  }

  return (
    <div className="mx-auto max-w-2xl p-6">
      <Link href="/employees" className="text-xs uppercase tracking-label text-muted hover:text-ink">
        ← Employees
      </Link>
      {error && (
        <p className="mt-4 text-m-red" role="alert" data-testid="detail-error">
          {error}
        </p>
      )}
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
            <ul className="mt-2 space-y-1 font-mono text-xs text-body" data-testid="image-list">
              {(emp.images || []).map((img) => (
                <li key={img.id}>
                  #{img.id}{" "}
                  {img.usable
                    ? "usable"
                    : `rejected${img.reject_reason ? ` — ${img.reject_reason}` : ""}`}
                </li>
              ))}
              {emp.image_count === 0 && <li className="text-muted">None yet</li>}
            </ul>
          </div>

          <form onSubmit={upload} className="mt-6 space-y-3">
            <label className="block text-[11px] font-bold uppercase tracking-label text-muted">
              Upload face images
              <input
                type="file"
                accept="image/*"
                multiple
                onChange={(e) => setFiles(e.target.files)}
                className="mt-2 block text-sm font-normal normal-case tracking-normal text-body"
                data-testid="detail-files"
              />
            </label>
            <div className="flex gap-3">
              <button
                type="submit"
                disabled={busy}
                className="border border-ink px-4 py-2 text-xs font-bold uppercase tracking-label hover:bg-elevated disabled:opacity-50"
                data-testid="upload-images"
              >
                Upload images
              </button>
              <button
                type="button"
                onClick={recompute}
                disabled={busy}
                className="border border-hairline px-4 py-2 text-xs font-bold uppercase tracking-label text-body hover:text-ink disabled:opacity-50"
                data-testid="recompute-embedding"
              >
                Recompute embedding
              </button>
            </div>
          </form>

          {statusMsg && (
            <p className="mt-3 text-sm text-success" role="status" data-testid="detail-status">
              {statusMsg}
            </p>
          )}

          {uploadResult?.gallery_reload_pending && (
            <p
              className="mt-2 text-xs text-warning"
              role="status"
              data-testid="gallery-pending"
            >
              Committed but converging — do not treat this as an upload error or
              resubmit the same files.
            </p>
          )}

          {resultRows.length > 0 && (
            <ul
              className="mt-3 space-y-1 font-mono text-xs text-body"
              data-testid="upload-results"
            >
              {resultRows.map((r) => (
                <li key={r.filename}>
                  <span className="text-ink">{r.filename}</span>
                  {": "}
                  {r.usable ? (
                    <span className="text-success">usable</span>
                  ) : (
                    <span className="text-m-red">
                      rejected{r.reason ? ` — ${r.reason}` : ""}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          )}

          {recomputeResult && (
            <p
              className="mt-2 font-mono text-xs text-muted"
              role="status"
              data-testid="recompute-result"
            >
              Recompute aggregate: usable={recomputeResult.usable} ready=
              {String(recomputeResult.embedding_ready)} used=
              {recomputeResult.num_images_used}
            </p>
          )}
        </>
      )}
    </div>
  );
}
