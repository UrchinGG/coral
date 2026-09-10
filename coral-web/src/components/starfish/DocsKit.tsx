export function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-10">
      <h2 className="text-lg font-semibold tracking-tight mb-3">{title}</h2>
      {children}
    </section>
  );
}

export function P({ children }: { children: React.ReactNode }) {
  return <p className="text-sm text-white/50 mb-3 leading-relaxed">{children}</p>;
}

export function Mono({ children }: { children: React.ReactNode }) {
  return <code className="text-[13px] text-white/70 bg-white/[0.06] px-1.5 py-0.5 rounded">{children}</code>;
}

export function Signature({ children }: { children: string }) {
  return (
    <div className="mb-3 px-3 py-2 rounded-md bg-white/[0.04] border border-white/[0.08] font-mono text-sm text-white/60">
      {children}
    </div>
  );
}

export function Code({ children }: { children: string }) {
  return (
    <pre className="mb-4 px-4 py-3 rounded-md bg-white/[0.04] border border-white/[0.08] overflow-x-auto">
      <code className="text-[13px] text-white/55 leading-relaxed">{children}</code>
    </pre>
  );
}

export function PropTable({ rows }: { rows: [string, string, string][] }) {
  return (
    <div className="mb-4 rounded-md border border-white/[0.08] overflow-hidden">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-white/[0.08] bg-white/[0.02]">
            <th className="text-left px-3 py-2 text-white/40 font-medium text-xs">Field</th>
            <th className="text-left px-3 py-2 text-white/40 font-medium text-xs">Type</th>
            <th className="text-left px-3 py-2 text-white/40 font-medium text-xs">Description</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([field, type, desc]) => (
            <tr key={field} className="border-b border-white/[0.04] last:border-0">
              <td className="px-3 py-2 font-mono text-[13px] text-white/60">{field}</td>
              <td className="px-3 py-2 text-[13px] text-white/40">{type}</td>
              <td className="px-3 py-2 text-[13px] text-white/40">{desc}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
