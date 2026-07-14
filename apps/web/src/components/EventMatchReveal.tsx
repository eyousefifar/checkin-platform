"use client";

import { useEffect, useId, useRef, useState } from "react";
import { snapshotAbsoluteUrl } from "@/lib/api";
import { timeFromEpochSeconds } from "@/lib/dateTime";
import type { AttendanceMsg } from "@/lib/types";

export type EventMatchRevealProps = {
  event: AttendanceMsg | null;
  timezone?: string | null;
  onClose: () => void;
};

const STAGES = ["ACQUIRE", "LOCK", "MATCH", "COMMIT"] as const;

export function EventMatchReveal({
  event,
  timezone,
  onClose,
}: EventMatchRevealProps) {
  const titleId = useId();
  const closeRef = useRef<HTMLButtonElement>(null);
  const [failedSnapshot, setFailedSnapshot] = useState<string | null>(null);
  const open = event != null;

  useEffect(() => {
    if (!open) return;
    const prev = document.activeElement as HTMLElement | null;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    closeRef.current?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        setFailedSnapshot(null);
        onClose();
      } else if (e.key === "Tab") {
        // The close control is intentionally the modal's only interactive
        // element; keep keyboard focus contained until the reveal is closed.
        e.preventDefault();
        closeRef.current?.focus();
      }
    }
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.body.style.overflow = previousOverflow;
      prev?.focus?.();
    };
  }, [open, onClose]);

  if (!event) return null;

  const imgSrc = event.snapshot_url
    ? snapshotAbsoluteUrl(event.snapshot_url)
    : null;
  const hasSnapshot = imgSrc != null && failedSnapshot !== imgSrc;
  const bbox = event.bbox;
  const confidence =
    event.score != null && Number.isFinite(event.score)
      ? `${(event.score * 100).toFixed(1)}%`
      : "—";

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/90 p-3 md:p-6"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      data-testid="event-match-reveal"
    >
      <div className="relative flex max-h-[min(92vh,900px)] w-full max-w-4xl flex-col border border-hairline bg-canvas">
        {/* Header telemetry */}
        <div className="flex min-h-11 flex-wrap items-center justify-between gap-2 border-b border-hairline px-4 py-2">
          <div className="min-w-0">
            <p
              id={titleId}
              className="truncate text-sm font-bold uppercase tracking-label text-ink"
              data-testid="reveal-name"
            >
              {event.name}
            </p>
            <p className="font-mono text-[10px] uppercase tracking-label text-muted">
              event {event.event_id} · match reveal
            </p>
          </div>
          <button
            ref={closeRef}
            type="button"
            onClick={() => {
              setFailedSnapshot(null);
              onClose();
            }}
            className="min-h-11 min-w-11 border border-hairline px-3 text-xs font-bold uppercase tracking-label text-body hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan"
            data-testid="reveal-close"
          >
            Close
          </button>
        </div>

        {/* Full-bleed stage */}
        <div
          className="relative flex min-h-[240px] flex-1 items-center justify-center overflow-hidden bg-soft"
          data-testid="reveal-stage"
        >
          {hasSnapshot && imgSrc ? (
            <div
              className="relative z-10 inline-block max-h-[68vh] max-w-full leading-none"
              data-testid="reveal-image-plane"
            >
              {/* Keep the target box in the image's own rendered coordinate
                  plane. This remains accurate when containment adds
                  letterboxing around non-16:9 camera frames. */}
              {/* eslint-disable-next-line @next/next/no-img-element */}
              <img
                src={imgSrc}
                alt={`Snapshot for ${event.name}`}
                onError={() => setFailedSnapshot(imgSrc)}
                className="block h-auto max-h-[68vh] w-auto max-w-full object-contain"
                data-testid="reveal-snapshot"
              />

              {bbox && (
                <div
                  className="pointer-events-none absolute"
                  data-testid="reveal-bbox"
                  style={{
                    left: `${bbox[0] * 100}%`,
                    top: `${bbox[1] * 100}%`,
                    width: `${(bbox[2] - bbox[0]) * 100}%`,
                    height: `${(bbox[3] - bbox[1]) * 100}%`,
                  }}
                >
                  <span className="absolute -left-px -top-px h-4 w-4 border-l-2 border-t-2 border-signal" />
                  <span className="absolute -right-px -top-px h-4 w-4 border-r-2 border-t-2 border-signal" />
                  <span className="absolute -bottom-px -left-px h-4 w-4 border-b-2 border-l-2 border-signal" />
                  <span className="absolute -bottom-px -right-px h-4 w-4 border-b-2 border-r-2 border-signal" />
                </div>
              )}
            </div>
          ) : (
            <div
              className="flex h-full min-h-[240px] items-center justify-center px-6 text-center"
              data-testid="reveal-unavailable"
            >
              <div>
                <p className="text-sm font-bold uppercase tracking-label text-warning">
                  Snapshot unavailable
                </p>
                <p className="mt-2 text-xs text-body">
                  This event was committed without a stored frame. Identity and
                  telemetry remain valid.
                </p>
              </div>
            </div>
          )}

          {/* Edge vignette */}
          <div
            className="pointer-events-none absolute inset-0 z-20 bg-gradient-to-t from-black/70 via-transparent to-black/40"
            aria-hidden="true"
          />

          {/* Animated scan sweep (disabled under reduced motion via CSS) */}
          <div
            className="reveal-scan pointer-events-none absolute inset-x-0 z-30 h-0.5 bg-cyan/70"
            aria-hidden="true"
            data-testid="reveal-scan"
          />

        </div>

        {/* Telemetry rail */}
        <div className="grid grid-cols-2 gap-px border-t border-hairline bg-hairline md:grid-cols-4">
          <Telemetry label="Kind" value={event.kind.replace("_", "-")} testId="reveal-kind" />
          <Telemetry label="Confidence" value={confidence} testId="reveal-confidence" />
          <Telemetry label="Camera" value={event.camera_id} testId="reveal-camera" />
          <Telemetry
            label="Time"
            value={timeFromEpochSeconds(event.ts, timezone)}
            testId="reveal-time"
          />
        </div>

        {/* Acquisition stages */}
        <ol
          className="flex flex-wrap items-center gap-2 border-t border-hairline px-4 py-3"
          data-testid="reveal-stages"
          aria-label="Acquisition stages"
        >
          {STAGES.map((s, i) => (
            <li
              key={s}
              className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-label text-signal"
            >
              {i > 0 && <span className="text-muted">›</span>}
              <span data-testid={`reveal-stage-${s.toLowerCase()}`}>{s}</span>
            </li>
          ))}
        </ol>
      </div>
    </div>
  );
}

function Telemetry({
  label,
  value,
  testId,
}: {
  label: string;
  value: string;
  testId: string;
}) {
  return (
    <div className="bg-canvas px-3 py-2">
      <p className="text-[10px] font-bold uppercase tracking-label text-muted">
        {label}
      </p>
      <p
        className="mt-0.5 font-mono text-xs uppercase text-ink"
        data-testid={testId}
      >
        {value}
      </p>
    </div>
  );
}
