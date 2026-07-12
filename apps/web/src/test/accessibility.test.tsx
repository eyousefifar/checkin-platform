import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/** Operational components must not use 9px text classes. */
const OPERATIONAL_SOURCES = [
  "src/components/AppShell.tsx",
  "src/components/CameraTile.tsx",
  "src/components/MetricPill.tsx",
  "src/components/EventTicker.tsx",
  "src/components/StatusBadge.tsx",
  "src/app/page.tsx",
  "src/app/employees/page.tsx",
  "src/app/attendance/page.tsx",
  "src/app/login/page.tsx",
];

describe("operational type scale", () => {
  it("has no text-[9px] on operational components", () => {
    const root = join(__dirname, "..", "..");
    for (const rel of OPERATIONAL_SOURCES) {
      const src = readFileSync(join(root, rel), "utf8");
      expect(src, rel).not.toMatch(/text-\[9px\]/);
    }
  });
});

describe("AppShell mobile navigation", () => {
  it("exposes labeled details menu with main links", async () => {
    vi.resetModules();
    vi.doMock("next/navigation", () => ({
      usePathname: () => "/",
    }));
    vi.doMock("next/link", () => ({
      default: ({
        href,
        children,
        ...rest
      }: {
        href: string;
        children: React.ReactNode;
      }) => (
        <a href={href} {...rest}>
          {children}
        </a>
      ),
    }));

    const { AppShell } = await import("@/components/AppShell");
    render(
      <AppShell>
        <div>child</div>
      </AppShell>,
    );

    const openControl = screen.getByLabelText(/open navigation menu/i);
    expect(openControl).toBeTruthy();

    const details = openControl.closest("details");
    expect(details).toBeTruthy();
    // Open via native property — closed details still keep children in the DOM.
    if (details) details.open = true;
    fireEvent.click(openControl);

    const mobileNav = screen.getByLabelText("Mobile");
    expect(within(mobileNav).getByRole("link", { name: "DASHBOARD" })).toBeTruthy();
    expect(within(mobileNav).getByRole("link", { name: "EMPLOYEES" })).toBeTruthy();
    expect(within(mobileNav).getByRole("link", { name: "ATTENDANCE" })).toBeTruthy();
    expect(within(mobileNav).getByRole("link", { name: "Admin" })).toBeTruthy();
  });
});

const empApiMock = vi.hoisted(() => vi.fn());
const attApiMock = vi.hoisted(() => vi.fn());
const attHealth = vi.hoisted(() => ({
  data: {
    status: "ok",
    timezone: "UTC",
    vision_ready: true,
    vision_provider: "mock",
    gallery_size: 0,
    cameras: [] as never[],
    media: {
      mediamtx_running: false,
      transcoder_running: false,
      publication: "unavailable",
      source_mode: null,
      preferred_webrtc_path: null,
      last_error: null,
    },
  },
  loading: false,
  error: null as string | null,
  refresh: () => {},
}));

describe("employees exclusive states", () => {
  beforeEach(() => {
    vi.resetModules();
    empApiMock.mockReset();
    vi.doMock("@/lib/api", () => ({
      api: (...args: unknown[]) => empApiMock(...args),
      API_URL: "http://localhost:8000",
    }));
    vi.doMock("next/link", () => ({
      default: ({
        href,
        children,
        ...rest
      }: {
        href: string;
        children: React.ReactNode;
      }) => (
        <a href={href} {...rest}>
          {children}
        </a>
      ),
    }));
  });

  it("loading suppresses empty and error", async () => {
    let resolve!: (v: unknown) => void;
    empApiMock.mockReturnValue(
      new Promise((r) => {
        resolve = r;
      }),
    );
    const EmployeesPage = (await import("@/app/employees/page")).default;
    render(<EmployeesPage />);
    expect(screen.getByTestId("employees-loading")).toBeTruthy();
    expect(screen.queryByTestId("employees-empty")).toBeNull();
    expect(screen.queryByTestId("employees-error")).toBeNull();
    resolve([]);
    await waitFor(() => {
      expect(screen.queryByTestId("employees-loading")).toBeNull();
    });
  });

  it("error suppresses empty state", async () => {
    empApiMock.mockRejectedValueOnce(new Error("boom"));
    const EmployeesPage = (await import("@/app/employees/page")).default;
    render(<EmployeesPage />);
    await waitFor(() => {
      expect(screen.getByTestId("employees-error")).toBeTruthy();
    });
    expect(screen.getByTestId("employees-error").getAttribute("role")).toBe(
      "alert",
    );
    expect(screen.queryByTestId("employees-empty")).toBeNull();
    expect(screen.queryByText(/No employees yet/i)).toBeNull();
    expect(screen.queryAllByTestId("employee-row")).toHaveLength(0);
  });

  it("empty shows only when loaded without error", async () => {
    empApiMock.mockResolvedValueOnce([]);
    const EmployeesPage = (await import("@/app/employees/page")).default;
    render(<EmployeesPage />);
    await waitFor(() => {
      expect(screen.getByTestId("employees-empty")).toBeTruthy();
    });
    expect(screen.queryByTestId("employees-error")).toBeNull();
    expect(screen.getByLabelText(/search employees/i)).toBeTruthy();
  });
});

describe("attendance filters and exclusive states", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date("2026-07-12T12:00:00.000Z"));
    attApiMock.mockReset();
    attHealth.loading = false;
    attHealth.error = null;
    attHealth.data.timezone = "UTC";
    vi.doMock("@/hooks/useHealth", () => ({
      useHealth: () => attHealth,
    }));
    vi.doMock("@/lib/api", () => ({
      API_URL: "http://localhost:8000",
      api: (...args: unknown[]) => attApiMock(...args),
      getToken: () => null,
    }));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("status chips expose aria-pressed", async () => {
    attApiMock.mockImplementation(async () => []);
    const AttendancePage = (await import("@/app/attendance/page")).default;
    render(<AttendancePage />);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /^all$/i })).toBeTruthy();
    });
    const all = screen.getByRole("button", { name: /^all$/i });
    expect(all.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: /^present$/i }));
    expect(
      screen.getByRole("button", { name: /^present$/i }).getAttribute("aria-pressed"),
    ).toBe("true");
    expect(all.getAttribute("aria-pressed")).toBe("false");
  });

  it("daily error suppresses empty rows message", async () => {
    attApiMock.mockImplementation(async (path: string) => {
      if (String(path).includes("/daily")) throw new Error("daily failed");
      return [];
    });
    const AttendancePage = (await import("@/app/attendance/page")).default;
    render(<AttendancePage />);
    await waitFor(() => {
      expect(screen.getByTestId("attendance-error")).toBeTruthy();
    });
    expect(screen.queryByTestId("attendance-empty")).toBeNull();
    expect(screen.queryByText(/No rows for this day/i)).toBeNull();
  });
});

describe("EventTicker live region", () => {
  it("uses polite status and does not announce detections", async () => {
    const { EventTicker } = await import("@/components/EventTicker");
    render(
      <EventTicker
        events={[
          {
            type: "attendance",
            event_id: 1,
            employee_id: 1,
            name: "Ada",
            kind: "check_in",
            camera_id: "cam_in",
            score: 0.9,
            ts: Math.floor(Date.parse("2026-07-12T12:00:00Z") / 1000),
          },
        ]}
        timezone="UTC"
      />,
    );
    const list = screen.getByTestId("event-ticker-list");
    expect(list.getAttribute("aria-live")).toBe("polite");
    expect(list.getAttribute("role")).toBe("status");
    expect(list.textContent).not.toMatch(/detection/i);
  });
});
