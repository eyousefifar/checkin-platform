import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MonoClock } from "./MonoClock";

describe("MonoClock", () => {
  afterEach(() => vi.useRealTimers());

  it("uses the configured timezone and updates no faster than once per second", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-14T12:00:00.000Z"));
    render(<MonoClock timezone="Asia/Tehran" />);

    const clock = screen.getByText("15:30:00");
    expect(clock.textContent).toMatch(/^\d{2}:\d{2}:\d{2}$/);
    expect(clock.textContent).not.toContain(".");

    act(() => vi.advanceTimersByTime(999));
    expect(clock.textContent).toBe("15:30:00");
    act(() => vi.advanceTimersByTime(1));
    expect(clock.textContent).toBe("15:30:01");
  });
});
