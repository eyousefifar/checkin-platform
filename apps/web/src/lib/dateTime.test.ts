import { afterEach, describe, expect, it, vi } from "vitest";
import {
  calendarDateInZone,
  dateTimeInZone,
  timeFromEpochSeconds,
  timeInZone,
} from "./dateTime";

describe("dateTime helpers", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("calendarDateInZone covers UTC, Tehran, and negative offset around midnight", () => {
    // 2026-07-12 22:30 UTC
    const lateUtc = new Date("2026-07-12T22:30:00.000Z");
    expect(calendarDateInZone(lateUtc, "UTC")).toBe("2026-07-12");
    // Asia/Tehran +03:30 → 2026-07-13 02:00
    expect(calendarDateInZone(lateUtc, "Asia/Tehran")).toBe("2026-07-13");
    // America/New_York UTC-4 in July → still 2026-07-12
    expect(calendarDateInZone(lateUtc, "America/New_York")).toBe("2026-07-12");

    // Just after UTC midnight: previous evening in New York
    const afterMidnight = new Date("2026-07-13T01:00:00.000Z");
    expect(calendarDateInZone(afterMidnight, "UTC")).toBe("2026-07-13");
    expect(calendarDateInZone(afterMidnight, "America/New_York")).toBe(
      "2026-07-12",
    );
    expect(calendarDateInZone(afterMidnight, "Asia/Tehran")).toBe("2026-07-13");
  });

  it("timeInZone formats known UTC instants in multiple zones", () => {
    const iso = "2026-07-12T20:00:00.000Z";
    expect(timeInZone(iso, "UTC")).toBe("20:00:00");
    expect(timeInZone(iso, "Asia/Tehran")).toBe("23:30:00");
    expect(timeInZone(iso, "America/New_York")).toBe("16:00:00");
  });

  it("timeInZone returns em dash for null/invalid without throwing", () => {
    expect(timeInZone(null, "UTC")).toBe("—");
    expect(timeInZone(undefined, "UTC")).toBe("—");
    expect(timeInZone("", "UTC")).toBe("—");
    expect(timeInZone("not-a-date", "UTC")).toBe("—");
    expect(timeInZone("2026-07-12T20:00:00.000Z", "Not/A_Zone")).toBe("—");
  });

  it("calendarDateInZone rejects invalid zone and invalid date", () => {
    expect(() => calendarDateInZone(new Date("bad"), "UTC")).toThrow();
    expect(() =>
      calendarDateInZone(new Date("2026-07-12T00:00:00.000Z"), "Not/A_Zone"),
    ).toThrow(/invalid timeZone/);
  });

  it("dateTimeInZone combines date and time", () => {
    expect(dateTimeInZone("2026-07-12T20:00:00.000Z", "Asia/Tehran")).toBe(
      "2026-07-12 23:30:00",
    );
    expect(dateTimeInZone(null, "UTC")).toBe("—");
  });

  it("uses fixed clock for calendar selection", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-12T22:30:00.000Z"));
    expect(calendarDateInZone(new Date(), "Asia/Tehran")).toBe("2026-07-13");
    expect(calendarDateInZone(new Date(), "UTC")).toBe("2026-07-12");
  });

  it("timeFromEpochSeconds formats WS timestamps without browser-local fallback", () => {
    // 2026-07-12T20:00:00Z
    const epoch = Math.floor(Date.parse("2026-07-12T20:00:00.000Z") / 1000);
    expect(timeFromEpochSeconds(epoch, "UTC")).toBe("20:00:00");
    expect(timeFromEpochSeconds(epoch, "Asia/Tehran")).toBe("23:30:00");
    expect(timeFromEpochSeconds(epoch, null)).toBe("—");
    expect(timeFromEpochSeconds(epoch, undefined)).toBe("—");
    expect(timeFromEpochSeconds(null, "UTC")).toBe("—");
    expect(timeFromEpochSeconds(Number.NaN, "UTC")).toBe("—");
  });
});
