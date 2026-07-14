"use client";

import { useEffect, useRef } from "react";
import type { FaceDet } from "@/lib/types";

export function FaceHudCanvas({
  faces,
  width,
  height,
}: {
  faces: FaceDet[];
  width: number;
  height: number;
}) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const physicalW = Math.max(1, Math.round(width * dpr));
    const physicalH = Math.max(1, Math.round(height * dpr));

    // Resize backing store only when physical size actually changes.
    if (canvas.width !== physicalW || canvas.height !== physicalH) {
      canvas.width = physicalW;
      canvas.height = physicalH;
    }
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);

    for (const f of faces) {
      const [x1, y1, x2, y2] = f.bbox;
      const x = x1 * width;
      const y = y1 * height;
      const w = (x2 - x1) * width;
      const h = (y2 - y1) * height;

      const known = !!f.employee_id && f.label !== "UNKNOWN" && f.label !== "AMBIGUOUS";
      const lowQ = !f.quality_ok;
      ctx.strokeStyle = lowQ
        ? "rgba(113,113,122,0.8)"
        : known
          ? "rgba(52,211,153,0.95)"
          : "rgba(244,244,245,0.75)";
      ctx.lineWidth = 1.5;
      ctx.strokeRect(x, y, w, h);

      // corner ticks
      const t = 8;
      ctx.beginPath();
      ctx.moveTo(x, y + t);
      ctx.lineTo(x, y);
      ctx.lineTo(x + t, y);
      ctx.stroke();

      const label = lowQ
        ? "LOW QUALITY"
        : `${f.label}${f.score ? ` ${f.score.toFixed(2)}` : ""}`;
      ctx.font = "11px ui-monospace, monospace";
      const tw = ctx.measureText(label).width + 8;
      ctx.fillStyle = "rgba(0,0,0,0.75)";
      ctx.fillRect(x, Math.max(0, y - 18), tw, 16);
      ctx.fillStyle = known ? "#e4e4e7" : "#85858f";
      ctx.fillText(label, x + 4, Math.max(12, y - 6));
    }
  }, [faces, width, height]);

  return (
    <canvas
      ref={ref}
      className="pointer-events-none absolute inset-0 z-10 h-full w-full"
    />
  );
}
