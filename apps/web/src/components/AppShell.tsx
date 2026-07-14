"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { CameraSessionsProvider } from "@/hooks/useCameraSessions";
import { HealthProvider, useHealth } from "@/hooks/useHealth";
import { LiveWsProvider } from "@/hooks/useLiveWs";
import { LicenseBanner } from "./LicenseBanner";
import { MonoClock } from "./MonoClock";

const NAV = [
  { href: "/", label: "MONITOR" },
  { href: "/employees", label: "CONFIGURE" },
  { href: "/attendance", label: "RECORDS" },
];

function navClass(active: boolean) {
  return `text-xs tracking-label transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan ${
    active
      ? "border-b border-cyan pb-0.5 font-semibold text-ink"
      : "text-body hover:text-ink"
  }`;
}

export function AppShell({ children }: { children: React.ReactNode }) {
  return (
    <HealthProvider>
      <LiveWsProvider>
        <CameraSessionsProvider>
          <AppShellContent>{children}</AppShellContent>
        </CameraSessionsProvider>
      </LiveWsProvider>
    </HealthProvider>
  );
}

function AppShellContent({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const { data: health } = useHealth();

  const systemOk = health?.status === "ok" && health?.vision_ready;
  const systemLabel = !health
    ? "SYS —"
    : systemOk
      ? "SYS NOMINAL"
      : "SYS DEGRADED";

  const links = NAV.map((item) => {
    const active =
      item.href === "/" ? pathname === "/" : pathname.startsWith(item.href);
    return (
      <Link
        key={item.href}
        href={item.href}
        className={navClass(active)}
        aria-current={active ? "page" : undefined}
      >
        {item.label}
      </Link>
    );
  });

  return (
    <div className="flex min-h-screen min-w-0 flex-col bg-canvas text-body-strong">
      <div className="tech-rule" aria-hidden="true" />
      <header className="flex h-14 min-w-0 items-center justify-between border-b border-hairline px-4 md:px-6">
        <div className="flex min-w-0 items-center gap-4 md:gap-8">
          <Link
            href="/"
            className="shrink-0 font-mono text-xs font-bold uppercase tracking-label text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
          >
            PKSP<span className="text-cyan">/</span>OPS
          </Link>
          <nav className="hidden items-center gap-6 md:flex" aria-label="Main">
            {links}
          </nav>
        </div>

        <div className="flex shrink-0 items-center gap-3 md:gap-4">
          <div
            className={`hidden font-mono text-[10px] uppercase tracking-label md:block ${
              systemOk ? "text-signal" : "text-warning"
            }`}
            data-testid="system-status"
            role="status"
          >
            {systemLabel}
          </div>
          <div className="hidden md:block" aria-hidden="true">
            <MonoClock timezone={health?.timezone ?? null} />
          </div>
          <Link
            href="/login"
            className="hidden text-xs uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan md:inline"
          >
            Auth
          </Link>

          <details className="relative md:hidden">
            <summary
              className="cursor-pointer list-none border border-hairline px-3 py-2 text-xs font-bold uppercase tracking-label text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan [&::-webkit-details-marker]:hidden"
              aria-label="Open navigation menu"
            >
              Menu
            </summary>
            <nav
              className="absolute right-0 z-50 mt-2 min-w-[12rem] border border-hairline bg-card py-2"
              aria-label="Mobile"
            >
              <div className="flex flex-col gap-1 px-3 py-1">
                {NAV.map((item) => {
                  const active =
                    item.href === "/"
                      ? pathname === "/"
                      : pathname.startsWith(item.href);
                  return (
                    <Link
                      key={item.href}
                      href={item.href}
                      className={`px-2 py-2 text-xs uppercase tracking-label focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan ${
                        active
                          ? "font-semibold text-ink"
                          : "text-body hover:text-ink"
                      }`}
                      aria-current={active ? "page" : undefined}
                    >
                      {item.label}
                    </Link>
                  );
                })}
                <Link
                  href="/login"
                  className="border-t border-hairline px-2 py-2 text-xs uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
                >
                  Auth
                </Link>
              </div>
            </nav>
          </details>
        </div>
      </header>
      <main className="min-w-0 flex-1">{children}</main>
      <LicenseBanner />
    </div>
  );
}
