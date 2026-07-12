"use client";

import { FormEvent, useState } from "react";
import { useRouter } from "next/navigation";
import { API_URL, setToken } from "@/lib/api";

export default function LoginPage() {
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const router = useRouter();

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setLoading(true);
    setError("");
    try {
      const res = await fetch(`${API_URL}/api/auth/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ password }),
      });
      if (!res.ok) throw new Error("Invalid password");
      const data = await res.json();
      setToken(data.access_token);
      router.push("/employees");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Login failed");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex min-h-[calc(100vh-7rem)] items-center justify-center p-6">
      <form
        onSubmit={onSubmit}
        className="w-full max-w-sm border border-hairline bg-card p-8"
      >
        <h1 className="text-xl font-bold uppercase tracking-wide text-ink">
          Admin login
        </h1>
        <p className="mt-2 text-sm text-body">LAN MVP · shared password</p>
        <label className="mt-6 block text-[11px] font-bold uppercase tracking-label text-muted">
          Password
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="mt-2 w-full border border-hairline bg-soft px-3 py-2 text-sm text-ink outline-none focus:border-m-blue-dark"
            autoFocus
          />
        </label>
        {error && <p className="mt-3 text-sm text-m-red">{error}</p>}
        <button
          type="submit"
          disabled={loading}
          className="mt-6 w-full border border-ink bg-canvas py-3 text-xs font-bold uppercase tracking-label text-ink hover:bg-elevated disabled:opacity-50"
        >
          {loading ? "…" : "Sign in"}
        </button>
      </form>
    </div>
  );
}
