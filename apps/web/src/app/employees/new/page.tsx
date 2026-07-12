"use client";

import { FormEvent, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { api } from "@/lib/api";

export default function NewEmployeePage() {
  const router = useRouter();
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [dept, setDept] = useState("");
  const [files, setFiles] = useState<FileList | null>(null);
  const [msg, setMsg] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    setMsg("");
    try {
      const emp = await api<{ id: number }>("/api/employees", {
        method: "POST",
        body: JSON.stringify({
          employee_code: code,
          full_name: name,
          department: dept || null,
        }),
      });
      if (files && files.length > 0) {
        const fd = new FormData();
        Array.from(files).forEach((f) => fd.append("files", f));
        const up = await api<{
          usable: number;
          rejected: { filename: string; reason: string }[];
          embedding_ready: boolean;
        }>(`/api/employees/${emp.id}/images`, { method: "POST", body: fd });
        setMsg(
          `Created. Usable ${up.usable}, embedding ${up.embedding_ready ? "ready" : "not ready"}. Rejected: ${up.rejected.map((r) => r.reason).join(", ") || "none"}`,
        );
      } else {
        setMsg("Created without images — upload faces on detail page.");
      }
      router.push(`/employees/${emp.id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed");
    } finally {
      setBusy(false);
    }
  }

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

      <form onSubmit={onSubmit} className="mt-6 space-y-4">
        <Field label="Employee code" value={code} onChange={setCode} required />
        <Field label="Full name" value={name} onChange={setName} required />
        <Field label="Department" value={dept} onChange={setDept} />
        <label className="block text-[11px] font-bold uppercase tracking-label text-muted">
          Face images
          <input
            type="file"
            accept="image/*"
            multiple
            onChange={(e) => setFiles(e.target.files)}
            className="mt-2 block w-full text-sm text-body"
          />
        </label>
        {error && <p className="text-sm text-m-red">{error}</p>}
        {msg && <p className="text-sm text-success">{msg}</p>}
        <button
          type="submit"
          disabled={busy}
          className="border border-ink px-6 py-3 text-xs font-bold uppercase tracking-label text-ink hover:bg-elevated disabled:opacity-50"
        >
          {busy ? "Saving…" : "Save"}
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
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  required?: boolean;
}) {
  return (
    <label className="block text-[11px] font-bold uppercase tracking-label text-muted">
      {label}
      <input
        value={value}
        required={required}
        onChange={(e) => onChange(e.target.value)}
        className="mt-2 w-full border border-hairline bg-card px-3 py-2 text-sm font-normal normal-case tracking-normal text-ink outline-none focus:border-m-blue-dark"
      />
    </label>
  );
}
