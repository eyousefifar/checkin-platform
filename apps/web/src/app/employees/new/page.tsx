"use client";

import { FormEvent, useEffect, useMemo, useState } from "react";
import Link from "next/link";
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

    if (selected.length === 0) {
      setBusy(false);
      return;
    }

    try {
      const fd = new FormData();
      selected.forEach((s) => fd.append("files", s.file));
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

  return (
    <div className="mx-auto max-w-xl p-6">
      <Link href="/employees" className="text-xs uppercase tracking-label text-muted hover:text-ink">
        ← Employees
      </Link>
      <h1 className="mt-4 text-2xl font-bold uppercase tracking-wide text-ink">
        Add employee
      </h1>

      <div className="mt-4 border border-hairline bg-card p-4 text-sm text-body">
        <p className="text-[11px] font-bold uppercase tracking-label text-muted">
          Photo guidance
        </p>
        <ul className="mt-2 list-inside list-disc space-y-1 text-xs">
          <li>Door-cam angle, even lighting, no heavy sunglasses</li>
          <li>One person per photo · 5–10 images better than one headshot</li>
        </ul>
      </div>

      {created && detailHref && (
        <div className="mt-6 space-y-3 border border-hairline bg-card p-4" role="status">
          <p className="text-sm text-success">
            Employee created (id {createdId}). Metadata will not be submitted again.
          </p>
          <p className="text-sm text-body">
            Open the detail page to review or retry photos:{" "}
            <Link href={detailHref} className="text-m-blue-light underline" data-testid="detail-link">
              Employee #{createdId}
            </Link>
          </p>

          {selected.length === 0 && !uploadResult && !uploadError && (
            <p className="text-sm text-body">
              Created without images — upload faces on the detail page.
            </p>
          )}

          {uploadError && (
            <div role="alert" className="text-sm text-m-red">
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
            </div>
          )}
        </div>
      )}

      <form onSubmit={onSubmit} className="mt-6 space-y-4">
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
        <label className="block text-[11px] font-bold uppercase tracking-label text-muted">
          Face images
          <input
            type="file"
            accept="image/*"
            multiple
            disabled={created || busy}
            onChange={(e) => onFilesChange(e.target.files)}
            className="mt-2 block w-full text-sm text-body disabled:opacity-50"
            data-testid="face-files"
          />
        </label>

        {selected.length > 0 && (
          <ul
            className="flex flex-wrap gap-3"
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
                <p className="truncate font-mono text-[10px] text-body" title={s.file.name}>
                  {s.file.name}
                </p>
              </li>
            ))}
          </ul>
        )}

        {createError && (
          <p className="text-sm text-m-red" role="alert">
            {createError}
          </p>
        )}

        <button
          type="submit"
          disabled={busy || created}
          className="border border-ink px-6 py-3 text-xs font-bold uppercase tracking-label text-ink hover:bg-elevated disabled:opacity-50"
          data-testid="save-employee"
        >
          {busy ? "Saving…" : created ? "Created" : "Save"}
        </button>
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
    <label className="block text-[11px] font-bold uppercase tracking-label text-muted">
      {label}
      <input
        value={value}
        required={required}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        className="mt-2 w-full border border-hairline bg-card px-3 py-2 text-sm font-normal normal-case tracking-normal text-ink outline-none focus:border-m-blue-dark disabled:opacity-50"
      />
    </label>
  );
}
