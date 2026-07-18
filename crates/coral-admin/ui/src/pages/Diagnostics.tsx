import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useSearchParams } from "react-router-dom";
import type { ColumnDef } from "@tanstack/react-table";
import {
  type LogFilters,
  type SeriesMode,
  useBudgets,
  usePaths,
  useRateLimits,
  useRequestLog,
  useSeries,
  useStats,
} from "../api/diagnostics";
import type { Bucket, BudgetRow, RequestRow, TopKey, TopPath } from "../api/types";
import { DataTable } from "../components/DataTable";
import { HealthSignal } from "../components/HealthSignal";
import { Identity } from "../components/Identity";
import { Panel } from "../components/Panel";
import { type Stat } from "../components/StatStrip";
import { RequestModal } from "../components/RequestModal";
import { StatusBar } from "../components/StatusBar";
import { TimeSeriesChart } from "../components/TimeSeriesChart";
import { STATUS_COLORS, fmtDate, fmtMs, fmtNum, fmtPercent } from "../format";

const WINDOWS: [number, string][] = [
  [1, "1h"],
  [6, "6h"],
  [24, "24h"],
  [72, "3d"],
  [168, "7d"],
  [336, "14d"],
];

const LOG_PAGE_SIZE = 50;
const LEADERBOARD_SIZE = 5;
const LEADERBOARD_EXPANDED_SIZE = 20;

const FILTER_KEYS = [
  "path",
  "path_exact",
  "method",
  "status",
  "key_prefix",
  "ip",
  "discord_id",
  "caller",
  "error_contains",
  "errors",
  "from",
  "to",
] as const;

function filtersFromParams(params: URLSearchParams): LogFilters {
  const hours = Number(params.get("hours")) || 24;
  return {
    hours,
    path: params.get("path") ?? undefined,
    path_exact: params.get("path_exact") === "true",
    method: params.get("method") ?? undefined,
    status: params.get("status") ?? undefined,
    key_prefix: params.get("key_prefix") ?? undefined,
    ip: params.get("ip") ?? undefined,
    discord_id: params.get("discord_id") ?? undefined,
    caller: params.get("caller") ?? undefined,
    error_contains: params.get("error_contains") ?? undefined,
    errors: params.get("errors") === "true",
    from: params.get("from") ? Number(params.get("from")) : undefined,
    to: params.get("to") ? Number(params.get("to")) : undefined,
  };
}

function computeHealth(hasData: boolean, errorRate: number, budgetPct: number) {
  if (!hasData) return { level: "ok" as const, message: "No traffic recorded yet" };
  if (errorRate > 0.1) return { level: "danger" as const, message: `Error rate elevated — ${fmtPercent(errorRate)}` };
  if (budgetPct > 0.9) return { level: "danger" as const, message: `Hypixel budget nearly exhausted — ${fmtPercent(budgetPct)} used` };
  if (errorRate > 0.03) return { level: "warning" as const, message: `Error rate slightly elevated — ${fmtPercent(errorRate)}` };
  if (budgetPct > 0.7) return { level: "warning" as const, message: `Hypixel budget usage climbing — ${fmtPercent(budgetPct)}` };
  return { level: "ok" as const, message: "All systems normal" };
}

const LOG_COLUMNS: ColumnDef<RequestRow, unknown>[] = [
  {
    header: "Time",
    accessorKey: "ts",
    cell: (info) => <span className="whitespace-nowrap text-gray-500">{fmtDate(info.getValue<string>())}</span>,
  },
  { header: "Method", accessorKey: "method", cell: (info) => info.getValue<string | null>() ?? "—" },
  {
    header: "Path",
    accessorKey: "path",
    cell: (info) => <span className="block max-w-[220px] truncate font-mono text-xs">{info.getValue<string | null>() ?? "—"}</span>,
  },
  { header: "Status", accessorKey: "status", cell: (info) => info.getValue<number | null>() ?? "—" },
  {
    header: "Latency",
    accessorKey: "latency_ms",
    cell: (info) => <span className="text-gray-500">{fmtMs(info.getValue<number | null>())}</span>,
  },
  {
    header: "Caller",
    id: "caller",
    cell: ({ row }) =>
      row.original.discord_id ? (
        <Identity id={row.original.discord_id} username={row.original.discord_username} />
      ) : row.original.uuid ? (
        <Identity id={row.original.uuid} username={row.original.minecraft_username} kind="minecraft" />
      ) : (
        <span className="text-gray-600">—</span>
      ),
  },
  {
    header: "IP",
    accessorKey: "ip",
    cell: (info) => <span className="font-mono text-xs">{info.getValue<string | null>() ?? "—"}</span>,
  },
];

export function Diagnostics() {
  const [searchParams, setSearchParams] = useSearchParams();
  const hours = Number(searchParams.get("hours")) || 24;
  const mode = (searchParams.get("mode") as SeriesMode | null) ?? "incoming";
  const endpoint = searchParams.get("endpoint") ?? "";
  const offset = Number(searchParams.get("offset")) || 0;
  const filters = useMemo(() => filtersFromParams(searchParams), [searchParams]);
  const [selected, setSelected] = useState<RequestRow | null>(null);

  const setParams = (patch: Record<string, string | undefined>) => {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        for (const [key, value] of Object.entries(patch)) {
          if (value === undefined || value === "") next.delete(key);
          else next.set(key, value);
        }
        return next;
      },
      { replace: true },
    );
  };

  const setFilters = (patch: Partial<LogFilters>, resetOffset = true) => {
    const entries: Record<string, string | undefined> = {};
    for (const key of Object.keys(patch) as (keyof LogFilters)[]) {
      const value = patch[key];
      entries[key] = value === undefined || value === false ? undefined : String(value);
    }
    if (resetOffset) entries.offset = undefined;
    setParams(entries);
  };

  const clearFilters = () => {
    const next = new URLSearchParams();
    next.set("hours", String(hours));
    setSearchParams(next, { replace: true });
  };

  const stats = useStats(hours);
  const rateLimits = useRateLimits();
  const budgets = useBudgets();
  const paths = usePaths(hours);
  const series = useSeries(mode, hours, endpoint);
  const log = useRequestLog(filters, offset, LOG_PAGE_SIZE);

  const errorRate = stats.data && stats.data.total > 0 ? stats.data.errors / stats.data.total : 0;
  const rps = stats.data ? stats.data.total / (hours * 3600) : 0;
  const budgetUsagePct = rateLimits.data?.capacity ? rateLimits.data.used / rateLimits.data.capacity : 0;
  const health = computeHealth(!!stats.data && stats.data.total > 0, errorRate, budgetUsagePct);

  const percentiles = useMemo(() => weightedLatency(stats.data?.top_paths ?? []), [stats.data]);

  const activeFilterCount = FILTER_KEYS.filter((k) => searchParams.get(k)).length;

  const railStats: Stat[] = [
    { label: "Requests", value: fmtNum(stats.data?.total ?? 0), sub: `${rps < 1 ? rps.toFixed(2) : Math.round(rps)}/s · ${hours}h` },
    {
      label: "Errors",
      value: fmtPercent(errorRate),
      sub: `${fmtNum(stats.data?.errors ?? 0)} total`,
      tone: errorRate > 0.05 ? "danger" : "default",
    },
    {
      label: "Avg latency",
      value: fmtMs(stats.data?.avg_ms ?? null),
      sub: percentiles ? `p95 ${fmtMs(percentiles.p95)} · p99 ${fmtMs(percentiles.p99)}` : "response time",
    },
    {
      label: "Hypixel headroom",
      value: rateLimits.data?.available ? fmtNum(rateLimits.data.headroom) : "—",
      sub: rateLimits.data?.available ? `${fmtNum(rateLimits.data.used)} / ${fmtNum(rateLimits.data.capacity)} used` : "redis offline",
      tone: budgetUsagePct > 0.85 ? "danger" : budgetUsagePct > 0.6 ? "warning" : "default",
    },
  ];

  const selectSlot = (bucket: Bucket, index: number, points: Bucket[]) => {
    const startMs = new Date(bucket.t).getTime();
    let width = points.length > 1 ? new Date(points[1].t).getTime() - new Date(points[0].t).getTime() : hours * 3600 * 1000;
    if (index < points.length - 1) {
      width = new Date(points[index + 1].t).getTime() - startMs;
    }
    setFilters({
      from: Math.floor(startMs / 1000),
      to: Math.floor(startMs / 1000) + Math.round(width / 1000),
    });
  };

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <HealthSignal level={health.level} message={health.message} />
        <Segmented value={hours} options={WINDOWS} onChange={(v) => setParams({ hours: String(v), offset: undefined })} />
      </div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-12">
        <Panel className="lg:col-span-8">
          <div className="mb-3 flex items-center gap-3">
            <Segmented
              value={mode}
              options={[
                ["incoming", "Incoming"],
                ["endpoint", "Endpoint"],
                ["hypixel", "Hypixel"],
              ]}
              onChange={(v) => setParams({ mode: v })}
            />
            {mode === "endpoint" && (
              <select
                className="rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
                value={endpoint}
                onChange={(e) => setParams({ endpoint: e.target.value })}
              >
                {(paths.data ?? []).map((p) => (
                  <option key={p.path ?? ""} value={p.path ?? ""}>
                    {p.path ?? "(none)"} · {fmtNum(p.count)}
                  </option>
                ))}
              </select>
            )}
          </div>
          <TimeSeriesChart
            points={series.data ?? []}
            hours={hours}
            totalLabel={mode === "hypixel" ? "Outgoing" : "Requests"}
            onSelect={mode === "hypixel" ? undefined : selectSlot}
          />
        </Panel>
        <StatRail stats={railStats} />
      </div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-3">
        <TopCallersLeaderboard keys={stats.data?.top_keys ?? []} onSelectCaller={(f) => setFilters(f)} />
        <TopEndpointsLeaderboard paths={stats.data?.top_paths ?? []} onSelectPath={(p) => setParams({ mode: "endpoint", endpoint: p })} />
        <BudgetHotspotsLeaderboard rows={budgets.data ?? []} onSelectCaller={(discordId) => setFilters({ discord_id: discordId })} />
      </div>

      <div className="rounded-2xl border border-white/8 bg-black/20 p-6">
        <div className="mb-4">
          <h2 className="text-base font-semibold text-gray-100">Investigate</h2>
          <p className="mt-0.5 text-xs text-gray-500">Full request log — combine filters to narrow down abuse or errors</p>
        </div>
        <LogPanel
          filters={filters}
          activeFilterCount={activeFilterCount}
          onApply={(f) => setFilters(f)}
          onClear={clearFilters}
          onClearOne={(key) => setFilters({ [key]: undefined } as Partial<LogFilters>)}
          total={log.data?.total ?? 0}
          requests={log.data?.requests ?? []}
          offset={offset}
          onOffsetChange={(o) => setParams({ offset: String(o) })}
          onSelectRequest={setSelected}
        />
      </div>

      {selected && <RequestModal request={selected} onClose={() => setSelected(null)} />}
    </div>
  );
}

function weightedLatency(topPaths: TopPath[]) {
  const withData = topPaths.filter((p) => p.p95_ms !== null);
  if (withData.length === 0) return null;
  const totalCount = withData.reduce((sum, p) => sum + p.count, 0);
  const p95 = withData.reduce((sum, p) => sum + (p.p95_ms ?? 0) * p.count, 0) / totalCount;
  const p99 = withData.reduce((sum, p) => sum + (p.p99_ms ?? 0) * p.count, 0) / totalCount;
  return { p95, p99 };
}

const RAIL_TONE_TEXT: Record<NonNullable<Stat["tone"]>, string> = {
  default: "text-gray-100",
  danger: "text-danger",
  warning: "text-warning",
  ok: "text-ok",
};

function StatRail({ stats }: { stats: Stat[] }) {
  return (
    <div className="flex flex-col divide-y divide-white/8 overflow-hidden rounded-xl border border-white/8 bg-white/[0.03] lg:col-span-4">
      {stats.map((s) => (
        <div key={s.label} className="flex flex-1 flex-col justify-center px-4 py-3">
          <div className="text-[11px] font-medium tracking-wide text-gray-500 uppercase">{s.label}</div>
          <div className={`mt-0.5 text-xl font-semibold ${RAIL_TONE_TEXT[s.tone ?? "default"]}`}>{s.value}</div>
          {s.sub && <div className="mt-0.5 text-xs text-gray-500">{s.sub}</div>}
        </div>
      ))}
    </div>
  );
}

function Segmented<T extends string | number>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: [T, string][];
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex overflow-hidden rounded-md border border-white/8">
      {options.map(([v, label]) => (
        <button
          key={String(v)}
          onClick={() => onChange(v)}
          className={`px-3 py-1.5 text-xs font-medium ${v === value ? "bg-accent/15 text-accent" : "text-gray-400 hover:bg-white/5"}`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function LeaderboardRow({ rank, left, right, onClick }: { rank: number; left: ReactNode; right: ReactNode; onClick?: () => void }) {
  return (
    <div
      className={`flex items-center gap-2 py-1.5 text-sm ${onClick ? "cursor-pointer hover:bg-white/4" : ""}`}
      onClick={onClick}
    >
      <span className="w-4 text-xs text-gray-600">{rank}</span>
      <span className="min-w-0 flex-1 truncate">{left}</span>
      <span className="shrink-0 text-xs text-gray-400">{right}</span>
    </div>
  );
}

function ShowAllToggle({ expanded, onToggle, hiddenCount }: { expanded: boolean; onToggle: () => void; hiddenCount: number }) {
  if (hiddenCount <= 0 && !expanded) return null;
  return (
    <button onClick={onToggle} className="mt-2 text-xs text-gray-500 hover:text-accent">
      {expanded ? "Show less" : `Show all (${hiddenCount} more) →`}
    </button>
  );
}

function TopCallersLeaderboard({ keys, onSelectCaller }: { keys: TopKey[]; onSelectCaller: (filters: Partial<LogFilters>) => void }) {
  const [expanded, setExpanded] = useState(false);
  const limit = expanded ? LEADERBOARD_EXPANDED_SIZE : LEADERBOARD_SIZE;
  const visible = keys.slice(0, limit);

  return (
    <Panel title="Top callers">
      {keys.length === 0 ? (
        <div className="text-sm text-gray-500">No data</div>
      ) : (
        <div className="flex flex-col divide-y divide-white/5">
          {visible.map((k, i) => (
            <LeaderboardRow
              key={k.key_prefix ?? k.discord_id ?? i}
              rank={i + 1}
              onClick={() =>
                onSelectCaller(k.discord_id ? { discord_id: k.discord_id } : k.key_prefix ? { key_prefix: k.key_prefix } : {})
              }
              left={
                k.discord_id ? (
                  <Identity id={k.discord_id} username={k.discord_username} />
                ) : k.uuid ? (
                  <Identity id={k.uuid} username={k.minecraft_username} kind="minecraft" />
                ) : (
                  <span className="font-mono text-xs text-gray-400">{k.key_prefix ?? "none"}</span>
                )
              }
              right={
                <span className="flex items-center gap-2">
                  {k.errors > 0 && <span className="text-danger">{fmtNum(k.errors)} err</span>}
                  {fmtNum(k.count)}
                </span>
              }
            />
          ))}
        </div>
      )}
      <ShowAllToggle expanded={expanded} onToggle={() => setExpanded((e) => !e)} hiddenCount={keys.length - LEADERBOARD_SIZE} />
    </Panel>
  );
}

function TopEndpointsLeaderboard({ paths, onSelectPath }: { paths: TopPath[]; onSelectPath: (path: string) => void }) {
  const [expanded, setExpanded] = useState(false);
  const limit = expanded ? LEADERBOARD_EXPANDED_SIZE : LEADERBOARD_SIZE;
  const visible = paths.slice(0, limit);

  return (
    <Panel title="Top endpoints">
      {paths.length === 0 ? (
        <div className="text-sm text-gray-500">No data</div>
      ) : (
        <div className="flex flex-col divide-y divide-white/5">
          {visible.map((p, i) => (
            <LeaderboardRow
              key={p.path ?? i}
              rank={i + 1}
              onClick={() => p.path && onSelectPath(p.path)}
              left={<span className="font-mono text-xs">{p.path ?? "—"}</span>}
              right={
                <span className="flex items-center gap-2">
                  <StatusBar
                    counts={[
                      { label: "2xx", count: p.status_2xx, color: STATUS_COLORS.s2xx },
                      { label: "3xx", count: p.status_3xx, color: STATUS_COLORS.s3xx },
                      { label: "4xx", count: p.status_4xx, color: STATUS_COLORS.s4xx },
                      { label: "5xx", count: p.status_5xx, color: STATUS_COLORS.s5xx },
                    ]}
                  />
                  {fmtNum(p.count)}
                </span>
              }
            />
          ))}
        </div>
      )}
      <ShowAllToggle expanded={expanded} onToggle={() => setExpanded((e) => !e)} hiddenCount={paths.length - LEADERBOARD_SIZE} />
    </Panel>
  );
}

function BudgetHotspotsLeaderboard({ rows, onSelectCaller }: { rows: BudgetRow[]; onSelectCaller: (discordId: string) => void }) {
  const [expanded, setExpanded] = useState(false);
  const active = useMemo(() => rows.filter((r) => r.used > 0).sort((a, b) => b.utilization - a.utilization), [rows]);
  const limit = expanded ? LEADERBOARD_EXPANDED_SIZE : LEADERBOARD_SIZE;
  const visible = active.slice(0, limit);

  return (
    <Panel title="Budget hotspots" description="Keyless session/batch rate limits">
      {active.length === 0 ? (
        <div className="text-sm text-gray-500">No active budgets</div>
      ) : (
        <div className="flex flex-col divide-y divide-white/5">
          {visible.map((r, i) => (
            <LeaderboardRow
              key={`${r.kind}:${r.discord_id}`}
              rank={i + 1}
              onClick={() => onSelectCaller(r.discord_id)}
              left={<Identity id={r.discord_id} username={r.discord_username} />}
              right={
                <span className={r.utilization > 0.85 ? "text-danger" : r.utilization > 0.6 ? "text-warning" : "text-gray-400"}>
                  {fmtPercent(r.utilization)}
                </span>
              }
            />
          ))}
        </div>
      )}
      <ShowAllToggle expanded={expanded} onToggle={() => setExpanded((e) => !e)} hiddenCount={active.length - LEADERBOARD_SIZE} />
    </Panel>
  );
}

function LogPanel({
  filters,
  activeFilterCount,
  onApply,
  onClear,
  onClearOne,
  total,
  requests,
  offset,
  onOffsetChange,
  onSelectRequest,
}: {
  filters: LogFilters;
  activeFilterCount: number;
  onApply: (f: Partial<LogFilters>) => void;
  onClear: () => void;
  onClearOne: (key: keyof LogFilters) => void;
  total: number;
  requests: RequestRow[];
  offset: number;
  onOffsetChange: (offset: number) => void;
  onSelectRequest: (r: RequestRow) => void;
}) {
  const [draft, setDraft] = useState({
    path: filters.path ?? "",
    path_exact: filters.path_exact ?? false,
    method: filters.method ?? "",
    status: filters.status ?? "",
    caller: filters.caller ?? "",
    error_contains: filters.error_contains ?? "",
    errors: filters.errors ?? false,
  });

  useEffect(() => {
    setDraft({
      path: filters.path ?? "",
      path_exact: filters.path_exact ?? false,
      method: filters.method ?? "",
      status: filters.status ?? "",
      caller: filters.caller ?? "",
      error_contains: filters.error_contains ?? "",
      errors: filters.errors ?? false,
    });
  }, [filters.path, filters.path_exact, filters.method, filters.status, filters.caller, filters.error_contains, filters.errors]);

  const applyDraft = () =>
    onApply({
      path: draft.path || undefined,
      path_exact: draft.path_exact,
      method: draft.method || undefined,
      status: draft.status || undefined,
      caller: draft.caller || undefined,
      error_contains: draft.error_contains || undefined,
      errors: draft.errors,
    });

  const chips: { key: keyof LogFilters; label: string }[] = [];
  if (filters.from && filters.to) {
    chips.push({
      key: "from",
      label: `time slot: ${new Date(filters.from * 1000).toLocaleTimeString()}–${new Date(filters.to * 1000).toLocaleTimeString()}`,
    });
  }
  if (filters.key_prefix) chips.push({ key: "key_prefix", label: `key: ${filters.key_prefix}` });
  if (filters.ip) chips.push({ key: "ip", label: `ip: ${filters.ip}` });
  if (filters.discord_id) chips.push({ key: "discord_id", label: `discord id: ${filters.discord_id}` });

  const clearChip = (key: keyof LogFilters) => {
    if (key === "from") onClearOne("to");
    onClearOne(key);
  };

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <select
          className="rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
          value={draft.method}
          onChange={(e) => setDraft({ ...draft, method: e.target.value })}
        >
          <option value="">any method</option>
          {["GET", "POST", "PATCH", "DELETE", "PUT"].map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
        <input
          className="rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
          placeholder="path…"
          value={draft.path}
          onChange={(e) => setDraft({ ...draft, path: e.target.value })}
          onKeyDown={(e) => e.key === "Enter" && applyDraft()}
        />
        <label className="flex items-center gap-1 text-xs text-gray-400">
          <input
            type="checkbox"
            checked={draft.path_exact}
            onChange={(e) => setDraft({ ...draft, path_exact: e.target.checked })}
          />
          exact
        </label>
        <input
          className="w-28 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
          placeholder="status: 429, 4xx…"
          value={draft.status}
          onChange={(e) => setDraft({ ...draft, status: e.target.value })}
          onKeyDown={(e) => e.key === "Enter" && applyDraft()}
        />
        <input
          className="w-48 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
          placeholder="caller: name, ID, UUID, or IP"
          value={draft.caller}
          onChange={(e) => setDraft({ ...draft, caller: e.target.value })}
          onKeyDown={(e) => e.key === "Enter" && applyDraft()}
        />
        <input
          className="w-40 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
          placeholder="error contains…"
          value={draft.error_contains}
          onChange={(e) => setDraft({ ...draft, error_contains: e.target.value })}
          onKeyDown={(e) => e.key === "Enter" && applyDraft()}
        />
        <label className="flex items-center gap-1 text-xs text-gray-400">
          <input
            type="checkbox"
            checked={draft.errors}
            onChange={(e) => setDraft({ ...draft, errors: e.target.checked })}
          />
          errors only
        </label>
        <button onClick={applyDraft} className="rounded-md bg-accent/15 px-3 py-1.5 text-xs font-medium text-accent hover:bg-accent/25">
          Apply
        </button>
        {activeFilterCount > 0 && (
          <button onClick={onClear} className="text-xs text-gray-500 hover:text-white">
            Clear all ({activeFilterCount})
          </button>
        )}
      </div>
      {chips.length > 0 && (
        <div className="mb-3 flex flex-wrap gap-2">
          {chips.map((c) => (
            <span key={c.key} className="rounded-full bg-white/8 px-2 py-0.5 text-xs text-gray-300">
              {c.label}{" "}
              <button onClick={() => clearChip(c.key)} className="text-gray-500 hover:text-white">
                ✕
              </button>
            </span>
          ))}
        </div>
      )}
      <div className="overflow-x-auto">
        <DataTable
          columns={LOG_COLUMNS}
          data={requests}
          onRowClick={onSelectRequest}
          rowClassName={(r) => ((r.status ?? 0) >= 400 ? "bg-danger/5" : "")}
          emptyMessage="No requests match"
        />
      </div>
      <div className="mt-4 flex items-center justify-between text-xs text-gray-500">
        <span>{total ? `${offset + 1}–${Math.min(offset + LOG_PAGE_SIZE, total)} of ${fmtNum(total)}` : "0 requests"}</span>
        <div className="flex gap-2">
          <button
            disabled={offset === 0}
            onClick={() => onOffsetChange(Math.max(0, offset - LOG_PAGE_SIZE))}
            className="rounded-md border border-white/10 px-2 py-1 disabled:opacity-40"
          >
            Prev
          </button>
          <button
            disabled={offset + LOG_PAGE_SIZE >= total}
            onClick={() => onOffsetChange(offset + LOG_PAGE_SIZE)}
            className="rounded-md border border-white/10 px-2 py-1 disabled:opacity-40"
          >
            Next
          </button>
        </div>
      </div>
    </div>
  );
}
