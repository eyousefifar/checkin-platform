import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { EventMatchReveal } from "./EventMatchReveal";
import type { AttendanceMsg } from "@/lib/types";

const withSnap: AttendanceMsg = {
  type: "attendance",
  event_id: 42,
  employee_id: 1,
  name: "Ada Lovelace",
  kind: "check_in",
  camera_id: "cam_in",
  score: 0.91,
  ts: Math.floor(Date.parse("2026-07-12T20:00:00.000Z") / 1000),
  snapshot_url: "/api/attendance/events/42/snapshot",
  bbox: [0.2, 0.3, 0.55, 0.7],
};

const noSnap: AttendanceMsg = {
  ...withSnap,
  event_id: 43,
  snapshot_url: null,
  bbox: null,
};

describe("EventMatchReveal", () => {
  it("renders nothing when event is null", () => {
    const { container } = render(
      <EventMatchReveal event={null} onClose={vi.fn()} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("shows snapshot, bbox placement, and telemetry", () => {
    render(
      <EventMatchReveal event={withSnap} timezone="UTC" onClose={vi.fn()} />,
    );
    expect(screen.getByTestId("event-match-reveal")).toBeTruthy();
    expect(screen.getByTestId("reveal-name").textContent).toContain("Ada");
    expect(screen.getByTestId("reveal-snapshot")).toBeTruthy();
    expect(screen.getByTestId("reveal-kind").textContent).toMatch(/check-in/i);
    expect(screen.getByTestId("reveal-confidence").textContent).toBe("91.0%");
    expect(screen.getByTestId("reveal-camera").textContent).toBe("cam_in");

    const box = screen.getByTestId("reveal-bbox");
    expect(box.parentElement).toBe(screen.getByTestId("reveal-snapshot").parentElement);
    expect(box.style.left).toBe("20%");
    expect(box.style.top).toBe("30%");
    expect(box.style.width).toBe("35%");
    expect(box.style.height).toBe("40%");
  });

  it("shows unavailable state when snapshot missing", () => {
    render(<EventMatchReveal event={noSnap} onClose={vi.fn()} />);
    expect(screen.getByTestId("reveal-unavailable")).toBeTruthy();
    expect(screen.queryByTestId("reveal-snapshot")).toBeNull();
    expect(screen.queryByTestId("reveal-bbox")).toBeNull();
  });

  it("falls back to unavailable when the stored snapshot cannot load", () => {
    render(<EventMatchReveal event={withSnap} onClose={vi.fn()} />);
    fireEvent.error(screen.getByTestId("reveal-snapshot"));

    expect(screen.getByTestId("reveal-unavailable")).toBeTruthy();
    expect(screen.queryByTestId("reveal-snapshot")).toBeNull();
    expect(screen.queryByTestId("reveal-bbox")).toBeNull();
  });

  it("retries the same snapshot after the reveal is closed and reopened", () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <EventMatchReveal event={withSnap} onClose={onClose} />,
    );
    fireEvent.error(screen.getByTestId("reveal-snapshot"));
    fireEvent.click(screen.getByTestId("reveal-close"));
    rerender(<EventMatchReveal event={null} onClose={onClose} />);
    rerender(<EventMatchReveal event={withSnap} onClose={onClose} />);

    expect(screen.getByTestId("reveal-snapshot")).toBeTruthy();
  });

  it("closes on Escape and close button", () => {
    const onClose = vi.fn();
    render(<EventMatchReveal event={withSnap} onClose={onClose} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId("reveal-close"));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("exposes dialog accessibility attributes", () => {
    render(<EventMatchReveal event={withSnap} onClose={vi.fn()} />);
    const dialog = screen.getByTestId("event-match-reveal");
    expect(dialog.getAttribute("role")).toBe("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.getAttribute("aria-labelledby")).toBeTruthy();
  });

  it("keeps keyboard focus inside the modal", () => {
    render(<EventMatchReveal event={withSnap} onClose={vi.fn()} />);
    const close = screen.getByTestId("reveal-close");
    expect(document.activeElement).toBe(close);
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(close);
  });

  it("renders acquisition stages", () => {
    render(<EventMatchReveal event={withSnap} onClose={vi.fn()} />);
    expect(screen.getByTestId("reveal-stage-acquire")).toBeTruthy();
    expect(screen.getByTestId("reveal-stage-match")).toBeTruthy();
    expect(screen.getByTestId("reveal-stage-commit")).toBeTruthy();
  });
});
