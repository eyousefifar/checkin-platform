"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { GuidedFaceCapture } from "@/components/GuidedFaceCapture";
import { api } from "@/lib/api";
import type { Employee, EnrollmentResult } from "@/lib/types";

export type EmployeeUpdateBody = {
  full_name: string;
  department: string | null;
  is_active: boolean;
};

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
  const [guidedFiles, setGuidedFiles] = useState<File[]>([]);
  const [busy, setBusy] = useState(false);

  // Edit form — reinitialized whenever server-confirmed employee changes.
  const [fullName, setFullName] = useState("");
  const [department, setDepartment] = useState("");
  const [isActive, setIsActive] = useState(true);

  /** Apply server-confirmed employee into form (load + successful PATCH only). */
  function applyServerEmployee(data: Employee) {
    setEmp(data);
    setFullName(data.full_name);
    setDepartment(data.department ?? "");
    setIsActive(data.is_active);
  }

  const load = useCallback(async () => {
    try {
      const data = await api<Employee>(`/api/employees/${id}`);
      applyServerEmployee(data);
      setError("");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed");
    }
  }, [id]);

  useEffect(() => {
    load();
  }, [load]);

  async function saveProfile(e: FormEvent) {
    e.preventDefault();
    if (!emp || busy) return;

    const nextActive = isActive;
    const wasActive = emp.is_active;
    if (wasActive && !nextActive) {
      const ok = window.confirm(
        "Deactivate this employee? Recognition will stop matching them, but attendance records and enrollment images remain. You can reactivate later.",
      );
      if (!ok) {
        // Restore the active control to the last server-confirmed value.
        setIsActive(true);
        return;
      }
    }

    setBusy(true);
    setError("");
    setStatusMsg("");
    const body: EmployeeUpdateBody = {
      full_name: fullName.trim(),
      department: department.trim() === "" ? null : department.trim(),
      is_active: nextActive,
    };
    try {
      const updated = await api<Employee>(`/api/employees/${id}`, {
        method: "PATCH",
        body: JSON.stringify(body),
      });
      // Only re-sync form from server on success — never wipe drafts on error.
      applyServerEmployee(updated);
      setStatusMsg("Employee profile saved.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed");
      // Retain last server-confirmed emp and the user's form values for retry.
    } finally {
      setBusy(false);
    }
  }

  const onGuidedChange = useCallback((list: File[]) => {
    setGuidedFiles(list);
  }, []);

  async function upload(e: FormEvent) {
    e.preventDefault();
    const manual = files ? Array.from(files) : [];
    const toUpload =
      guidedFiles.length > 0
        ? [
            ...guidedFiles,
            ...manual.filter((f) => !guidedFiles.some((g) => g.name === f.name)),
          ]
        : manual;
    if (!toUpload.length) return;
    setBusy(true);
    setError("");
    setStatusMsg("");
    setUploadResult(null);
    setRecomputeResult(null);
    try {
      const fd = new FormData();
      toUpload.forEach((f) => fd.append("files", f));
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
    <div className="mx-auto min-w-0 max-w-2xl p-4 md:p-6">
      <Link
        href="/employees"
        className="text-sm uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
      >
        ← Employees
      </Link>
      {error && (
        <p className="mt-4 text-danger" role="alert" data-testid="detail-error">
          {error}
        </p>
      )}
      {emp && (
        <>
          <p className="mt-4 text-[10px] font-bold uppercase tracking-label text-muted">
            Configure
          </p>
          <h1 className="text-2xl font-bold uppercase tracking-wide text-ink">
            {emp.full_name}
          </h1>
          <p className="mt-1 font-mono text-sm text-body">
            <span data-testid="employee-code-display">{emp.employee_code}</span>
            {" · "}
            {emp.department || "no dept"} · embedding{" "}
            <span className={emp.embedding_ready ? "text-signal" : "text-warning"}>
              {emp.embedding_ready ? "ready" : "missing"}
            </span>
            {" · "}
            <span data-testid="employee-active-display">
              {emp.is_active ? "active" : "inactive"}
            </span>
          </p>

          <form
            onSubmit={saveProfile}
            className="mt-6 space-y-3 border border-hairline bg-card p-4"
            data-testid="profile-form"
          >
            <h2 className="text-sm font-bold uppercase tracking-label text-body">
              Profile
            </h2>
            <div>
              <label
                htmlFor="employee-code"
                className="block text-sm font-bold uppercase tracking-label text-body"
              >
                Employee code
              </label>
              <input
                id="employee-code"
                type="text"
                value={emp.employee_code}
                readOnly
                disabled
                className="mt-1 w-full border border-hairline bg-soft px-3 py-2 font-mono text-sm text-muted"
                data-testid="employee-code-readonly"
              />
            </div>
            <div>
              <label
                htmlFor="full-name"
                className="block text-sm font-bold uppercase tracking-label text-body"
              >
                Full name
              </label>
              <input
                id="full-name"
                type="text"
                value={fullName}
                onChange={(e) => setFullName(e.target.value)}
                disabled={busy}
                required
                className="mt-1 w-full border border-hairline bg-elevated px-3 py-2 text-sm text-ink"
                data-testid="edit-full-name"
              />
            </div>
            <div>
              <label
                htmlFor="department"
                className="block text-sm font-bold uppercase tracking-label text-body"
              >
                Department
              </label>
              <input
                id="department"
                type="text"
                value={department}
                onChange={(e) => setDepartment(e.target.value)}
                disabled={busy}
                className="mt-1 w-full border border-hairline bg-elevated px-3 py-2 text-sm text-ink"
                data-testid="edit-department"
              />
            </div>
            <div className="flex items-center gap-2">
              <input
                id="is-active"
                type="checkbox"
                checked={isActive}
                onChange={(e) => setIsActive(e.target.checked)}
                disabled={busy}
                className="h-4 w-4"
                data-testid="edit-is-active"
              />
              <label htmlFor="is-active" className="text-sm text-body">
                Active (recognition enabled)
              </label>
            </div>
            <button
              type="submit"
              disabled={busy}
              className="min-h-11 border border-ink px-4 py-2 text-sm font-bold uppercase tracking-label hover:bg-elevated focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan disabled:opacity-50"
              data-testid="save-profile"
            >
              Save profile
            </button>
          </form>

          <div className="mt-6 border border-hairline bg-card p-4">
            <h2 className="text-sm font-bold uppercase tracking-label text-body">
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

          <div className="mt-6">
            <GuidedFaceCapture
              onCapturedChange={onGuidedChange}
              disabled={busy}
            />
          </div>

          <form onSubmit={upload} className="mt-6 space-y-3">
            <label
              htmlFor="detail-files"
              className="block text-sm font-bold uppercase tracking-label text-body"
            >
              Manual upload fallback
            </label>
            <input
              id="detail-files"
              type="file"
              accept="image/*"
              multiple
              onChange={(e) => setFiles(e.target.files)}
              className="mt-2 block text-sm font-normal normal-case tracking-normal text-body"
              data-testid="detail-files"
            />
            <div className="flex flex-wrap gap-3">
              <button
                type="submit"
                disabled={busy}
                className="min-h-11 border border-ink px-4 py-2 text-sm font-bold uppercase tracking-label hover:bg-elevated focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan disabled:opacity-50"
                data-testid="upload-images"
              >
                Upload images
              </button>
              <button
                type="button"
                onClick={recompute}
                disabled={busy}
                className="min-h-11 border border-hairline px-4 py-2 text-sm font-bold uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan disabled:opacity-50"
                data-testid="recompute-embedding"
              >
                Recompute embedding
              </button>
            </div>
          </form>

          {statusMsg && (
            <p className="mt-3 text-sm text-signal" role="status" data-testid="detail-status">
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
                    <span className="text-signal">usable</span>
                  ) : (
                    <span className="text-danger">
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
