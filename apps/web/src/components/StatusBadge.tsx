const STYLES: Record<string, string> = {
  present: "border-signal/40 text-signal",
  incomplete: "border-warning/40 text-warning",
  absent: "border-hairline text-muted",
  anomaly: "border-danger/40 text-danger",
};

export function StatusBadge({ status }: { status: string }) {
  const cls = STYLES[status] || STYLES.absent;
  return (
    <span
      className={`inline-block border px-2 py-0.5 text-xs font-bold uppercase tracking-label ${cls}`}
    >
      {status}
    </span>
  );
}
