"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { LicenseBanner } from "./LicenseBanner";
import { MonoClock } from "./MonoClock";

const NAV = [
  { href: "/", label: "DASHBOARD" },
  { href: "/employees", label: "EMPLOYEES" },
  { href: "/attendance", label: "ATTENDANCE" },
];

export function AppShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  return (
    <div className="flex min-h-screen flex-col bg-canvas text-body-strong">
      <div className="m-stripe" />
      <header className="flex h-16 items-center justify-between border-b border-hairline px-6">
        <div className="flex items-center gap-8">
          <Link
            href="/"
            className="text-sm font-bold uppercase tracking-label text-ink"
          >
            PKSP CHECK-IN
          </Link>
          <nav className="flex gap-6">
            {NAV.map((item) => {
              const active =
                item.href === "/"
                  ? pathname === "/"
                  : pathname.startsWith(item.href);
              return (
                <Link
                  key={item.href}
                  href={item.href}
                  className={`text-sm tracking-wide transition-colors ${
                    active
                      ? "border-b-2 border-m-blue-dark pb-0.5 font-semibold text-ink"
                      : "text-body hover:text-ink"
                  }`}
                >
                  {item.label}
                </Link>
              );
            })}
          </nav>
        </div>
        <div className="flex items-center gap-4">
          <MonoClock />
          <Link
            href="/login"
            className="text-xs uppercase tracking-label text-muted hover:text-ink"
          >
            Admin
          </Link>
        </div>
      </header>
      <main className="flex-1">{children}</main>
      <LicenseBanner />
    </div>
  );
}
