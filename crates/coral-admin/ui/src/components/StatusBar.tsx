type StatusBarProps = {
  counts: { label: string; count: number; color: string }[];
};

export function StatusBar({ counts }: StatusBarProps) {
  const total = counts.reduce((sum, c) => sum + c.count, 0);
  if (total === 0) {
    return <span className="text-xs text-gray-500">—</span>;
  }
  return (
    <div className="flex h-2 w-24 overflow-hidden rounded-full bg-white/5">
      {counts
        .filter((c) => c.count > 0)
        .map((c) => (
          <div
            key={c.label}
            className={c.color}
            style={{ width: `${(c.count / total) * 100}%` }}
            title={`${c.label}: ${c.count.toLocaleString()}`}
          />
        ))}
    </div>
  );
}
