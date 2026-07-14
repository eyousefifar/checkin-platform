"use client";

import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { GuidedFaceCapture } from "@/components/GuidedFaceCapture";
import { api } from "@/lib/api";
import type { EnrollmentResult } from "@/lib/types";

type SelectedPreview = {
  file: File;
  url: string;
};

export default function NewEmployeePage() {
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [dept, setDept] = useState("");
  const [selected, setSelected] = useState<SelectedPreview[]>([]);
  const [guidedFiles, setGuidedFiles] = useState<File[]>([]);
  const [createError, setCreateError] = useState("");
  const [uploadError, setUploadError] = useState("");
  const [uploadResult, setUploadResult] = useState<EnrollmentResult | null>(null);
  const [createdId, setCreatedId] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);

  // Revoke every object URL on selection change and unmount.
  useEffect(() => {
    return () => {
      selected.forEach((s) => URL.revokeObjectURL(s.url));
    };
  }, [selected]);

  const created = createdId != null;

  const onGuidedChange = useCallback((files: File[]) => {
    setGuidedFiles(files);
  }, []);

  function onFilesChange(list: FileList | null) {
    setSelected((prev) => {
      prev.forEach((s) => URL.revokeObjectURL(s.url));
      if (!list) return [];
      return Array.from(list).map((file) => ({
        file,
        url: URL.createObjectURL(file),
      }));
    });
  }

  /** Prefer guided captures; fall back / merge with manual files (manual after guided). */
  function filesForUpload(): File[] {
    if (guidedFiles.length > 0 && selected.length === 0) return guidedFiles;
    if (selected.length > 0 && guidedFiles.length === 0) {
      return selected.map((s) => s.file);
    }
    // Both: guided first, then unique manual by name
    const names = new Set(guidedFiles.map((f) => f.name));
    return [
      ...guidedFiles,
      ...selected.map((s) => s.file).filter((f) => !names.has(f.name)),
    ];
  }

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    // Metadata create is the commit point — never repeat after success.
    if (createdId != null) return;

    setBusy(true);
    setCreateError("");
    setUploadError("");
    setUploadResult(null);

    let empId: number;
    try {
      const emp = await api<{ id: number }>("/api/employees", {
        method: "POST",
        body: JSON.stringify({
          employee_code: code,
          full_name: name,
          department: dept || null,
        }),
      });
      empId = emp.id;
      setCreatedId(empId);
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : "Failed to create employee");
      setBusy(false);
      return;
    }

    const toUpload = filesForUpload();
    if (toUpload.length === 0) {
      setBusy(false);
      return;
    }

    try {
      const fd = new FormData();
      toUpload.forEach((f) => fd.append("files", f));
      const up = await api<EnrollmentResult>(`/api/employees/${empId}/images`, {
        method: "POST",
        body: fd,
      });
      setUploadResult(up);
    } catch (err) {
      setUploadError(
        err instanceof Error
          ? err.message
          : "Image upload failed",
      );
    } finally {
      setBusy(false);
    }
  }

  const detailHref = createdId != null ? `/employees/${createdId}` : null;

  const resultRows = useMemo(() => {
    if (!uploadResult) return [];
    if (uploadResult.results && uploadResult.results.length > 0) {
      return uploadResult.results;
    }
    // Fall back to rejected list when per-file results are absent.
    return uploadResult.rejected.map((r) => ({
      filename: r.filename,
      usable: false,
      reason: r.reason,
    }));
  }, [uploadResult]);

  const hasImages = guidedFiles.length > 0 || selected.length > 0;

  return (
    <div className="min-w-0 p-4 md:p-6">
      <Link
        href="/employees"
        className="text-sm uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
      >
        ← Employees
      </Link>
      <div className="mt-4 flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="text-[10px] font-bold uppercase tracking-label text-muted">
            Configure
          </p>
          <h1 className="text-2xl font-bold uppercase tracking-wide text-ink">
            Add employee
          </h1>
        </div>
        <p className="font-mono text-[10px] uppercase tracking-label text-muted">
          enrollment · pose-guided
        </p>
      </div>

      {created && detailHref && (
        <div className="mt-6 space-y-3 border border-hairline bg-card p-4" role="status">
          <p className="text-sm text-signal">
            Employee created (id {createdId}). Metadata will not be submitted again.
          </p>
          <p className="text-sm text-body">
            Open the detail page to review or retry photos:{" "}
            <Link
              href={detailHref}
              className="text-ink underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
              data-testid="detail-link"
            >
              Employee #{createdId}
            </Link>
          </p>

          {!hasImages && !uploadResult && !uploadError && (
            <p className="text-sm text-body">
              Created without images — upload faces on the detail page.
            </p>
          )}

          {uploadError && (
            <div role="alert" className="text-sm text-danger">
              Employee exists, but photo upload failed: {uploadError}. Retry images
              from the detail page — do not create the employee again.
            </div>
          )}

          {uploadResult && (
            <div className="space-y-2 text-sm text-body">
              <p>
                Upload: usable {uploadResult.usable}/{uploadResult.received}, embedding{" "}
                {uploadResult.embedding_ready ? "ready" : "not ready"}
                {uploadResult.gallery_reload_pending
                  ? " · recognition is catching up (gallery reload pending)"
                  : ""}
              </p>
              {uploadResult.gallery_reload_pending && (
                <p className="text-xs text-warning" data-testid="gallery-pending">
                  Images committed; recognition will use them after the gallery reloads.
                  Do not resubmit these files.
                </p>
              )}
              {resultRows.length > 0 && (
                <ul className="space-y-1 font-mono text-xs" data-testid="upload-results">
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
            </div>
          )}
        </div>
      )}

      <form onSubmit={onSubmit} className="mt-6">
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
          {/* Camera dominant column */}
          <div className="min-w-0 space-y-4 lg:col-span-7">
            <GuidedFaceCapture
              onCapturedChange={onGuidedChange}
              disabled={created || busy}
            />

            <div className="border border-hairline bg-card p-3">
              <label
                htmlFor="face-files"
                className="block text-xs font-bold uppercase tracking-label text-muted"
              >
                Manual upload fallback
              </label>
              <input
                id="face-files"
                type="file"
                accept="image/*"
                multiple
                disabled={created || busy}
                onChange={(e) => onFilesChange(e.target.files)}
                className="mt-2 block w-full text-sm text-body disabled:opacity-50"
                data-testid="face-files"
              />

              {selected.length > 0 && (
                <ul
                  className="mt-3 flex flex-wrap gap-3"
                  data-testid="file-previews"
                  aria-label="Selected face image previews"
                >
                  {selected.map((s) => (
                    <li key={s.url} className="w-20 space-y-1">
                      {/* eslint-disable-next-line @next/next/no-img-element */}
                      <img
                        src={s.url}
                        alt=""
                        className="h-16 w-16 border border-hairline object-cover"
                      />
                      <p className="truncate font-mono text-xs text-body" title={s.file.name}>
                        {s.file.name}
                      </p>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>

          {/* Form secondary column */}
          <div className="min-w-0 space-y-4 lg:col-span-5">
            <Field
              label="Employee code"
              value={code}
              onChange={setCode}
              required
              disabled={created || busy}
            />
            <Field
              label="Full name"
              value={name}
              onChange={setName}
              required
              disabled={created || busy}
            />
            <Field
              label="Department"
              value={dept}
              onChange={setDept}
              disabled={created || busy}
            />

            {createError && (
              <p className="text-sm text-danger" role="alert">
                {createError}
              </p>
            )}

            <button
              type="submit"
              disabled={busy || created}
              className="min-h-11 w-full border border-ink px-6 py-3 text-sm font-bold uppercase tracking-label text-ink hover:bg-elevated focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan disabled:opacity-50"
              data-testid="save-employee"
            >
              {busy ? "Saving…" : created ? "Created" : "Save"}
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}

function Field({
  label,
  value,
  onChange,
  required,
  disabled,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  required?: boolean;
  disabled?: boolean;
}) {
  return (
    <label className="block text-sm font-bold uppercase tracking-label text-body">
      {label}
      <input
        value={value}
        required={required}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        className="mt-2 w-full border border-hairline bg-card px-3 py-2 text-sm font-normal normal-case tracking-normal text-ink outline-none focus:border-cyan focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan disabled:opacity-50"
      />
    </label>
  );
}
