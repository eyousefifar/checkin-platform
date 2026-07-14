import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DailyRow, RawAttendanceEvent } from "@/lib/types";

const healthState = vi.hoisted(() => ({
  data: null as null | {
    status: string;
    timezone: string;
    vision_ready: boolean;
    vision_model: "buffalo_l";
    vision_provider: string;
    gallery_size: number;
    cameras: never[];
    media: {
      mediamtx_running: boolean;
      transcoder_running: boolean;
      publication: string;
      source_mode: null;
      preferred_webrtc_path: null;
      last_error: null;
    };
  },
  loading: true,
  error: null as string | null,
  refresh: () => {},
}));

vi.mock("@/hooks/useHealth", () => ({
  useHealth: () => healthState,
}));

const apiMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/api", () => ({
  API_URL: "http://localhost:8000",
  api: (...args: unknown[]) => apiMock(...args),
  getToken: () => null,
  snapshotAbsoluteUrl: (path: string) => `http://localhost:8000${path}`,
}));

import AttendancePage from "./page";

function healthPayload(timezone: string) {
  return {
    status: "ok",
    timezone,
    vision_ready: true,
    vision_model: "buffalo_l" as const,
    vision_provider: "test",
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
  };
}

const dailyRows: DailyRow[] = [
  {
    employee_id: 1,
    employee_code: "E1",
    full_name: "Ada Lovelace",
    department: "Eng",
    first_in: "2026-07-12T05:00:00Z",
    last_out: "2026-07-12T13:00:00Z",
    duration_minutes: 480,
    status: "present",
    check_in_count: 1,
    check_out_count: 1,
  },
  {
    employee_id: 2,
    employee_code: "E2",
    full_name: "Bob",
    department: null,
    first_in: "2026-07-12T06:00:00Z",
    last_out: null,
    duration_minutes: null,
    status: "incomplete",
    check_in_count: 1,
    check_out_count: 0,
  },
];

/** API returns newest-first; UI must show oldest-to-newest in detail. */
const rawEvents: RawAttendanceEvent[] = [
  {
    id: 20,
    employee_id: 1,
    camera_id: "cam_out",
    kind: "check_out",
    score: 0.91,
    ts: "2026-07-12T13:00:00Z",
    local_date: "2026-07-12",
  },
  {
    id: 10,
    employee_id: 1,
    camera_id: "cam_in",
    kind: "check_in",
    score: 0.88,
    ts: "2026-07-12T05:00:00Z",
    local_date: "2026-07-12",
    snapshot_url: "/api/attendance/events/10/snapshot",
    bbox: [0.2, 0.2, 0.6, 0.7],
  },
  {
    id: 11,
    employee_id: 2,
    camera_id: "cam_in",
    kind: "check_in",
    score: null,
    ts: "2026-07-12T06:00:00Z",
    local_date: "2026-07-12",
  },
];

function routeApi(impl?: {
  daily?: DailyRow[] | Error;
  events?: RawAttendanceEvent[] | Error;
  delayEventsMs?: number;
}) {
  apiMock.mockImplementation(async (path: string) => {
    if (typeof path === "string" && path.includes("/api/attendance/daily")) {
      const v = impl?.daily ?? dailyRows;
      if (v instanceof Error) throw v;
      return v;
    }
    if (typeof path === "string" && path.includes("/api/attendance/events")) {
      if (impl?.delayEventsMs) {
        await new Promise((r) => setTimeout(r, impl.delayEventsMs));
      }
      const v = impl?.events ?? rawEvents;
      if (v instanceof Error) throw v;
      return v;
    }
    throw new Error(`unexpected path ${path}`);
  });
}

describe("Attendance explainability", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date("2026-07-12T10:00:00.000Z"));
    healthState.data = healthPayload("UTC");
    healthState.loading = false;
    healthState.error = null;
    apiMock.mockReset();
    routeApi();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("types a raw event fixture without casts", () => {
    const fixture: RawAttendanceEvent = {
      id: 1,
      employee_id: null,
      camera_id: "cam_in",
      kind: "check_in",
      score: 0.5,
      ts: "2026-07-12T08:00:00Z",
      local_date: "2026-07-12",
    };
    expect(fixture.employee_id).toBeNull();
    expect(fixture.camera_id).toBe("cam_in");
  });

  it("issues exactly two requests per date and no per-row event fetch", async () => {
    render(<AttendancePage />);
    await waitFor(() => {
      expect(apiMock).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(screen.getByText("Ada Lovelace")).toBeTruthy();
    });
    const paths = apiMock.mock.calls.map((c) => c[0] as string);
    expect(paths).toHaveLength(2);
    expect(paths).toContain("/api/attendance/daily?date=2026-07-12");
    expect(paths).toContain("/api/attendance/events?date=2026-07-12");
    expect(paths.some((p) => p.includes("employee_id="))).toBe(false);
  });

  it("expands a row by accessible name and shows ordered fields", async () => {
    render(<AttendancePage />);
    await waitFor(() => expect(screen.getByText("Ada Lovelace")).toBeTruthy());

    const expand = screen.getByTestId("expand-1");
    expect(expand.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(expand);
    expect(expand.getAttribute("aria-expanded")).toBe("true");

    const detail = screen.getByTestId("detail-1");
    const items = within(detail).getAllByTestId(/raw-event-/);
    expect(items).toHaveLength(2);
    // Oldest first: check_in then check_out
    expect(items[0].getAttribute("data-testid")).toBe("raw-event-10");
    expect(items[1].getAttribute("data-testid")).toBe("raw-event-20");
    expect(within(items[0]).getByText("05:00:00")).toBeTruthy();
    expect(within(items[0]).getByText("check-in")).toBeTruthy();
    expect(within(items[0]).getByText("cam_in")).toBeTruthy();
    expect(within(items[0]).getByText("0.88")).toBeTruthy();
    expect(within(items[1]).getByText("cam_out")).toBeTruthy();

    fireEvent.click(expand);
    expect(expand.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByTestId("detail-1")).toBeNull();
  });

  it("formats event times in configured timezone", async () => {
    healthState.data = healthPayload("Asia/Tehran");
    render(<AttendancePage />);
    await waitFor(() => expect(screen.getByText("Ada Lovelace")).toBeTruthy());
    fireEvent.click(screen.getByTestId("expand-1"));
    // 05:00 UTC → 08:30 Tehran
    expect(within(screen.getByTestId("raw-event-10")).getByText("08:30:00")).toBeTruthy();
  });

  it("opens the sci-fi match reveal for a stored historical event", async () => {
    render(<AttendancePage />);
    await waitFor(() => expect(screen.getByText("Ada Lovelace")).toBeTruthy());
    fireEvent.click(screen.getByTestId("expand-1"));

    fireEvent.click(
      within(screen.getByTestId("raw-event-10")).getByRole("button", {
        name: /inspect event 10/i,
      }),
    );

    expect(screen.getByTestId("event-match-reveal")).toBeTruthy();
    expect(screen.getByTestId("reveal-name").textContent).toContain("Ada Lovelace");
    expect(screen.getByTestId("reveal-snapshot")).toBeTruthy();
    expect(screen.getByTestId("reveal-bbox")).toBeTruthy();
  });

  it("shows per-row empty without pairing error", async () => {
    routeApi({
      daily: dailyRows,
      events: [rawEvents[0], rawEvents[1]], // only employee 1
    });
    render(<AttendancePage />);
    await waitFor(() => expect(screen.getByText("Bob")).toBeTruthy());
    fireEvent.click(screen.getByTestId("expand-2"));
    expect(screen.getByTestId("detail-empty-2").textContent).toMatch(/No events/);
    expect(screen.queryByTestId("detail-error-2")).toBeNull();
  });

  it("raw event failure keeps daily rows and does not claim no events", async () => {
    routeApi({
      daily: dailyRows,
      events: new Error("events 500"),
    });
    render(<AttendancePage />);
    await waitFor(() => expect(screen.getByText("Ada Lovelace")).toBeTruthy());
    expect(screen.getByTestId("events-load-error").textContent).toMatch(
      /Raw events unavailable/,
    );
    fireEvent.click(screen.getByTestId("expand-1"));
    const err = screen.getByTestId("detail-error-1");
    expect(err.textContent).toMatch(/Could not load events/);
    expect(screen.queryByTestId("detail-empty-1")).toBeNull();
    // Aggregate still visible.
    expect(screen.getByText("Ada Lovelace")).toBeTruthy();
  });

  it("ignores stale responses when date changes mid-flight", async () => {
    let resolveSlow: ((v: RawAttendanceEvent[]) => void) | null = null;
    const slowEvents = new Promise<RawAttendanceEvent[]>((res) => {
      resolveSlow = res;
    });

    apiMock.mockImplementation(async (path: string) => {
      if (path.includes("/api/attendance/daily?date=2026-07-12")) {
        return dailyRows;
      }
      if (path.includes("/api/attendance/events?date=2026-07-12")) {
        return slowEvents;
      }
      if (path.includes("/api/attendance/daily?date=2026-07-11")) {
        return [
          {
            ...dailyRows[0],
            employee_id: 99,
            full_name: "Later Day",
            employee_code: "L99",
          },
        ];
      }
      if (path.includes("/api/attendance/events?date=2026-07-11")) {
        return [] as RawAttendanceEvent[];
      }
      throw new Error(path);
    });

    render(<AttendancePage />);
    await waitFor(() => {
      expect(apiMock.mock.calls.some((c) => String(c[0]).includes("daily"))).toBe(
        true,
      );
    });

    const dateInput = screen.getByLabelText(/date/i) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(dateInput, { target: { value: "2026-07-11" } });
    });

    await waitFor(() => {
      expect(screen.getByText("Later Day")).toBeTruthy();
    });

    // Resolve stale events for the old date — must not clobber.
    await act(async () => {
      resolveSlow?.(rawEvents);
      await Promise.resolve();
    });

    expect(screen.getByText("Later Day")).toBeTruthy();
    expect(screen.queryByText("Ada Lovelace")).toBeNull();
    // Expanding should show empty for the new day, not stale Ada events.
    fireEvent.click(screen.getByTestId("expand-99"));
    expect(screen.getByTestId("detail-empty-99")).toBeTruthy();
  });

  it("shows em dash for null score", async () => {
    render(<AttendancePage />);
    await waitFor(() => expect(screen.getByText("Bob")).toBeTruthy());
    fireEvent.click(screen.getByTestId("expand-2"));
    const item = screen.getByTestId("raw-event-11");
    expect(within(item).getByText("—")).toBeTruthy();
  });
});
