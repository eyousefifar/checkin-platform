import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { EventTicker } from "./EventTicker";
import type { AttendanceMsg } from "@/lib/types";

const sample: AttendanceMsg = {
  type: "attendance",
  event_id: 7,
  employee_id: 1,
  name: "Ada",
  kind: "check_in",
  camera_id: "cam_in",
  score: 0.93,
  // 2026-07-12T20:00:00Z
  ts: Math.floor(Date.parse("2026-07-12T20:00:00.000Z") / 1000),
};

describe("EventTicker", () => {
  it("renders polite live region for attendance events only", () => {
    render(<EventTicker events={[sample]} timezone="UTC" />);
    const list = screen.getByTestId("event-ticker-list");
    expect(list.getAttribute("role")).toBe("status");
    expect(list.getAttribute("aria-live")).toBe("polite");
    expect(list.getAttribute("aria-relevant")).toBe("additions");
  });

  it("shows local time and camera when timezone is provided", () => {
    render(<EventTicker events={[sample]} timezone="Asia/Tehran" />);
    // 20:00 UTC → 23:30 Tehran
    expect(screen.getByTestId("event-ticker-time").textContent).toBe("23:30:00");
    expect(screen.getByTestId("event-ticker-camera").textContent).toBe("cam_in");
    expect(screen.getByText("Ada")).toBeTruthy();
    expect(screen.getByText(/check-in/i)).toBeTruthy();
    expect(screen.getByText("0.93")).toBeTruthy();
  });

  it("renders em dash for time when timezone is unavailable", () => {
    render(<EventTicker events={[sample]} timezone={null} />);
    expect(screen.getByTestId("event-ticker-time").textContent).toBe("—");
    expect(screen.getByTestId("event-ticker-camera").textContent).toBe("cam_in");
    expect(screen.getByText("Ada")).toBeTruthy();
  });

  it("renders em dash when timezone prop is omitted", () => {
    render(<EventTicker events={[sample]} />);
    expect(screen.getByTestId("event-ticker-time").textContent).toBe("—");
  });
});
