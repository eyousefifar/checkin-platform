/**
 * Native timezone helpers for APP_TIMEZONE calendar selection and display.
 * Uses Intl only — no date library, no browser-local fallback for business dates.
 */

const DISPLAY_LOCALE = "en-US";

function partsMap(
  date: Date,
  timeZone: string,
  options: Intl.DateTimeFormatOptions,
): Map<string, string> {
  const fmt = new Intl.DateTimeFormat(DISPLAY_LOCALE, { timeZone, ...options });
  const map = new Map<string, string>();
  for (const p of fmt.formatToParts(date)) {
    if (p.type !== "literal") map.set(p.type, p.value);
  }
  return map;
}

function assertValidTimeZone(timeZone: string): void {
  if (!timeZone || typeof timeZone !== "string") {
    throw new Error("invalid timeZone");
  }
  // Throws RangeError for unknown IANA names.
  try {
    new Intl.DateTimeFormat(DISPLAY_LOCALE, { timeZone }).format(new Date(0));
  } catch {
    throw new Error(`invalid timeZone: ${timeZone}`);
  }
}

/**
 * Calendar date `YYYY-MM-DD` for `instant` in the named IANA zone.
 * Builds from named parts — never locale string ordering.
 */
export function calendarDateInZone(instant: Date, timeZone: string): string {
  if (!(instant instanceof Date) || Number.isNaN(instant.getTime())) {
    throw new Error("invalid instant");
  }
  assertValidTimeZone(timeZone);
  const parts = partsMap(instant, timeZone, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
  const y = parts.get("year");
  const m = parts.get("month");
  const d = parts.get("day");
  if (!y || !m || !d) {
    throw new Error("calendar parts missing");
  }
  return `${y}-${m}-${d}`;
}

/**
 * Wall-clock `HH:MM:SS` for a UTC ISO wire timestamp in the named IANA zone.
 * Invalid timestamps/timezones return an em dash rather than browser-local time.
 */
export function timeInZone(isoUtc: string | null | undefined, timeZone: string): string {
  if (isoUtc == null || isoUtc === "") return "—";
  try {
    assertValidTimeZone(timeZone);
    const d = new Date(isoUtc);
    if (Number.isNaN(d.getTime())) return "—";
    const parts = partsMap(d, timeZone, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
    // Some engines emit hour "24" for midnight; normalize to 00.
    let hour = parts.get("hour") ?? "00";
    if (hour === "24") hour = "00";
    const minute = parts.get("minute") ?? "00";
    const second = parts.get("second") ?? "00";
    return `${hour.padStart(2, "0")}:${minute.padStart(2, "0")}:${second.padStart(2, "0")}`;
  } catch {
    return "—";
  }
}

/**
 * Combined local date + time for future explainability UIs (plan 017).
 * Returns `YYYY-MM-DD HH:MM:SS` or em dash on failure.
 */
export function dateTimeInZone(
  isoUtc: string | null | undefined,
  timeZone: string,
): string {
  if (isoUtc == null || isoUtc === "") return "—";
  try {
    const d = new Date(isoUtc);
    if (Number.isNaN(d.getTime())) return "—";
    const date = calendarDateInZone(d, timeZone);
    const time = timeInZone(isoUtc, timeZone);
    if (time === "—") return "—";
    return `${date} ${time}`;
  } catch {
    return "—";
  }
}

/**
 * Format a WS epoch-seconds timestamp as `HH:MM:SS` in the named zone.
 * Invalid/missing values return em dash — never browser-local time.
 */
export function timeFromEpochSeconds(
  epochSeconds: number | null | undefined,
  timeZone: string | null | undefined,
): string {
  if (timeZone == null || timeZone === "") return "—";
  if (epochSeconds == null || !Number.isFinite(epochSeconds)) return "—";
  // Accept seconds; reject values that look like milliseconds by magnitude only
  // if absurd — WS contract is seconds.
  const ms = epochSeconds * 1000;
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "—";
  return timeInZone(d.toISOString(), timeZone);
}
