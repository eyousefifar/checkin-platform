import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

describe("AttendancePage timezone", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date("2026-07-12T22:30:00.000Z"));
    healthState.data = null;
    healthState.loading = true;
    healthState.error = null;
    apiMock.mockReset();
    // Daily + raw events load in parallel per selected date.
    apiMock.mockImplementation(async (path: unknown) => {
      if (typeof path === "string" && path.includes("/api/attendance/")) {
        return [];
      }
      return [];
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("waits for health and does not query daily with a guessed date", () => {
    render(<AttendancePage />);
    expect(screen.getByTestId("attendance-health-loading")).toBeTruthy();
    expect(apiMock).not.toHaveBeenCalled();
  });

  it("first daily request uses configured calendar date; later health does not reset selection", async () => {
    healthState.loading = false;
    healthState.data = healthPayload("Asia/Tehran");

    const { rerender } = render(<AttendancePage />);

    await waitFor(() => {
      expect(apiMock).toHaveBeenCalled();
    });
    // 22:30 UTC → 2026-07-13 in Asia/Tehran — daily + events in parallel.
    const initialPaths = apiMock.mock.calls.map((c) => c[0] as string);
    expect(initialPaths).toContain("/api/attendance/daily?date=2026-07-13");
    expect(initialPaths).toContain("/api/attendance/events?date=2026-07-13");
    expect(screen.getByTestId("attendance-timezone").textContent).toBe(
      "Asia/Tehran",
    );

    const dateInput = screen.getByLabelText(/date/i) as HTMLInputElement;
    expect(dateInput.value).toBe("2026-07-13");

    // User picks another day.
    await act(async () => {
      fireEvent.change(dateInput, { target: { value: "2026-07-10" } });
    });

    await waitFor(() => {
      const paths = apiMock.mock.calls.map((c) => c[0] as string);
      expect(paths).toContain("/api/attendance/daily?date=2026-07-10");
      expect(paths).toContain("/api/attendance/events?date=2026-07-10");
    });

    const callsBeforeRefresh = apiMock.mock.calls.length;

    // Health refresh with a different timezone must not reset user date.
    healthState.data = healthPayload("UTC");
    rerender(<AttendancePage />);

    await act(async () => {
      await Promise.resolve();
    });

    const dateAfter = screen.getByLabelText(/date/i) as HTMLInputElement;
    expect(dateAfter.value).toBe("2026-07-10");
    // May re-render but should keep querying the user-selected date if it refetches.
    const dateCalls = apiMock.mock.calls
      .slice(callsBeforeRefresh)
      .map((c) => c[0] as string);
    for (const c of dateCalls) {
      expect(c).toContain("date=2026-07-10");
    }
  });

  it("renders known UTC instants as local times in the configured zone", async () => {
    healthState.loading = false;
    healthState.data = healthPayload("Asia/Tehran");
    apiMock.mockImplementation(async (path: unknown) => {
      if (typeof path === "string" && path.includes("/events")) return [];
      return [
        {
          employee_id: 1,
          employee_code: "E1",
          full_name: "Ada",
          department: null,
          first_in: "2026-07-12T20:00:00Z",
          last_out: "2026-07-12T20:00:00Z",
          duration_minutes: 0,
          status: "present",
          check_in_count: 1,
          check_out_count: 1,
        },
      ];
    });

    render(<AttendancePage />);

    await waitFor(() => {
      expect(screen.getByTestId("first-in-1").textContent).toBe("23:30:00");
    });
    expect(screen.getByTestId("last-out-1").textContent).toBe("23:30:00");
  });

  it("formats the same UTC wire values for America/New_York", async () => {
    healthState.loading = false;
    healthState.data = healthPayload("America/New_York");
    apiMock.mockImplementation(async (path: unknown) => {
      if (typeof path === "string" && path.includes("/events")) return [];
      return [
        {
          employee_id: 2,
          employee_code: "E2",
          full_name: "Bob",
          department: null,
          first_in: "2026-07-12T20:00:00Z",
          last_out: null,
          duration_minutes: null,
          status: "incomplete",
          check_in_count: 1,
          check_out_count: 0,
        },
      ];
    });

    render(<AttendancePage />);

    await waitFor(() => {
      expect(screen.getByTestId("first-in-2").textContent).toBe("16:00:00");
    });
    expect(screen.getByTestId("last-out-2").textContent).toBe("—");
  });
});
