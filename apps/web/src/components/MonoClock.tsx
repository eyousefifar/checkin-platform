"use client";

import { useEffect, useState } from "react";

export function MonoClock() {
  const [t, setT] = useState("--:--:--.---");
  useEffect(() => {
    const tick = () => {
      const d = new Date();
      const pad = (n: number, w = 2) => String(n).padStart(w, "0");
      setT(
        `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`,
      );
    };
    tick();
    const id = setInterval(tick, 50);
    return () => clearInterval(id);
  }, []);
  return (
    <span className="font-mono text-xs tabular-nums tracking-wider text-muted">
      {t}
    </span>
  );
}
