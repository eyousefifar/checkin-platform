"use client";

import { useEffect, useState } from "react";
import { timeInZone } from "@/lib/dateTime";

export function MonoClock({ timezone }: { timezone: string | null }) {
  const [t, setT] = useState("--:--:--");
  useEffect(() => {
    if (!timezone) {
      setT("--:--:--");
      return;
    }
    const tick = () => {
      setT(timeInZone(new Date().toISOString(), timezone));
    };
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [timezone]);
  return (
    <span className="font-mono text-xs tabular-nums tracking-wider text-muted">
      {t}
    </span>
  );
}
