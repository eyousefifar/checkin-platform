"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { CameraSessionsProvider } from "@/hooks/useCameraSessions";
import { HealthProvider, useHealth } from "@/hooks/useHealth";
import { LiveWsProvider } from "@/hooks/useLiveWs";
import { LicenseBanner } from "./LicenseBanner";
import { MonoClock } from "./MonoClock";

const NAV = [
  { href: "/", label: "DASHBOARD" },
  { href: "/employees", label: "EMPLOYEES" },
  { href: "/attendance", label: "ATTENDANCE" },
];

function navClass(active: boolean) {
  return `text-sm tracking-wide transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark ${
    active
      ? "border-b-2 border-m-blue-dark pb-0.5 font-semibold text-ink"
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
      <div className="m-stripe" />
      <header className="flex h-16 min-w-0 items-center justify-between border-b border-hairline px-4 md:px-6">
        <div className="flex min-w-0 items-center gap-4 md:gap-8">
          <Link
            href="/"
            className="shrink-0 text-sm font-bold uppercase tracking-label text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark"
          >
            PKSP CHECK-IN
          </Link>
          <nav className="hidden items-center gap-6 md:flex" aria-label="Main">
            {links}
          </nav>
        </div>

        <div className="flex shrink-0 items-center gap-3 md:gap-4">
          <div className="hidden md:block" aria-hidden="true">
            <MonoClock timezone={health?.timezone ?? null} />
          </div>
          <Link
            href="/login"
            className="hidden text-sm uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark md:inline"
          >
            Admin
          </Link>

          {/* Native mobile navigation — no menu library */}
          <details className="relative md:hidden">
            <summary
              className="cursor-pointer list-none border border-hairline px-3 py-2 text-sm font-bold uppercase tracking-label text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark [&::-webkit-details-marker]:hidden"
              aria-label="Open navigation menu"
            >
              Menu
            </summary>
            <nav
              className="absolute right-0 z-50 mt-2 min-w-[12rem] border border-hairline bg-card py-2 shadow-lg"
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
                      className={`px-2 py-2 text-sm uppercase tracking-wide focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark ${
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
                  className="border-t border-hairline px-2 py-2 text-sm uppercase tracking-wide text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-m-blue-dark"
                >
                  Admin
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
