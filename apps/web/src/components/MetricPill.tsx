export function MetricPill({
  label,
  value,
}: {
  label: string;
  value: string | number;
}) {
  return (
    <div className="border border-hairline bg-card px-4 py-3">
      <div className="text-[10px] font-bold uppercase tracking-label text-muted">
        {label}
      </div>
      <div className="mt-1 font-mono text-2xl text-ink">{value}</div>
    </div>
  );
}
