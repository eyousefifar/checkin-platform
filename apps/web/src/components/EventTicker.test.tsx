import { fireEvent, render, screen } from "@testing-library/react";
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
  snapshot_url: "/api/attendance/events/7/snapshot",
  bbox: [0.1, 0.1, 0.4, 0.5],
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

  it("opens match reveal when an event row is activated", () => {
    render(<EventTicker events={[sample]} timezone="UTC" />);
    const item = screen.getByTestId("event-ticker-item");
    expect(item.tagName).toBe("BUTTON");
    fireEvent.click(item);
    expect(screen.getByTestId("event-match-reveal")).toBeTruthy();
    expect(screen.getByTestId("reveal-name").textContent).toContain("Ada");
  });

  it("restores focus when a new live event arrives behind the reveal", () => {
    const { rerender } = render(<EventTicker events={[sample]} timezone="UTC" />);
    const trigger = screen.getByRole("button", { name: /inspect event 7/i });
    trigger.focus();
    fireEvent.click(trigger);

    rerender(
      <EventTicker
        events={[
          { ...sample, event_id: 8, name: "Grace Hopper" },
          sample,
        ]}
        timezone="UTC"
      />,
    );
    fireEvent.click(screen.getByTestId("reveal-close"));

    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: /inspect event 7/i }),
    );
  });
});
