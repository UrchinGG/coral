import { useEffect, useMemo, useState } from "react";
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
import { Card } from "../components/Card";
import { DataTable } from "../components/DataTable";
import { Identity } from "../components/Identity";
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
    cell: (info) => <span className="block max-w-[200px] truncate font-mono text-xs">{info.getValue<string | null>() ?? "—"}</span>,
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

  const activeFilterCount = FILTER_KEYS.filter((k) => searchParams.get(k)).length;

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
    <div className="flex flex-col gap-5">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold">API Diagnostics</h1>
        <Segmented
          value={hours}
          options={WINDOWS}
          onChange={(v) => setParams({ hours: String(v), offset: undefined })}
        />
      </div>

      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Card label="Requests" value={fmtNum(stats.data?.total ?? 0)} sub={`${rps < 1 ? rps.toFixed(2) : Math.round(rps)}/s · ${hours}h`} />
        <Card
          label="Errors"
          value={fmtPercent(errorRate)}
          sub={`${fmtNum(stats.data?.errors ?? 0)} total`}
          tone={errorRate > 0.05 ? "danger" : "default"}
        />
        <LatencyCard topPaths={stats.data?.top_paths ?? []} avgMs={stats.data?.avg_ms ?? null} />
        <BudgetCard
          available={rateLimits.data?.available ?? false}
          headroom={rateLimits.data?.headroom ?? 0}
          used={rateLimits.data?.used ?? 0}
          capacity={rateLimits.data?.capacity ?? 0}
          pct={budgetUsagePct}
        />
      </div>

      <div className="rounded-lg border border-white/10 bg-white/5 p-4">
        <div className="mb-3 flex items-center justify-between gap-3">
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
              className="rounded border border-white/10 bg-black/30 px-2 py-1 text-sm"
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
      </div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <TopCallersPanel
          keys={stats.data?.top_keys ?? []}
          onSelectCaller={(f) => setFilters(f)}
        />
        <TopEndpointsPanel
          paths={stats.data?.top_paths ?? []}
          onSelectPath={(p) => setParams({ mode: "endpoint", endpoint: p })}
        />
      </div>

      <BudgetsPanel rows={budgets.data ?? []} onSelectCaller={(discordId) => setFilters({ discord_id: discordId })} />

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

      {selected && <RequestModal request={selected} onClose={() => setSelected(null)} />}
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
    <div className="flex overflow-hidden rounded border border-white/10">
      {options.map(([v, label]) => (
        <button
          key={String(v)}
          onClick={() => onChange(v)}
          className={`px-3 py-1.5 text-sm ${
            v === value ? "bg-white/15 text-white" : "text-gray-400 hover:bg-white/5"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function LatencyCard({ topPaths, avgMs }: { topPaths: TopPath[]; avgMs: number | null }) {
  const weighted = useMemo(() => {
    const withData = topPaths.filter((p) => p.p95_ms !== null);
    if (withData.length === 0) return null;
    const totalCount = withData.reduce((sum, p) => sum + p.count, 0);
    const p95 = withData.reduce((sum, p) => sum + (p.p95_ms ?? 0) * p.count, 0) / totalCount;
    const p99 = withData.reduce((sum, p) => sum + (p.p99_ms ?? 0) * p.count, 0) / totalCount;
    return { p95, p99 };
  }, [topPaths]);

  return (
    <Card
      label="Avg latency"
      value={fmtMs(avgMs)}
      sub={weighted ? `p95 ${fmtMs(weighted.p95)} · p99 ${fmtMs(weighted.p99)}` : "response time"}
    />
  );
}

function BudgetCard({
  available,
  headroom,
  used,
  capacity,
  pct,
}: {
  available: boolean;
  headroom: number;
  used: number;
  capacity: number;
  pct: number;
}) {
  return (
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="text-xs uppercase tracking-wide text-gray-400">Hypixel headroom</div>
      <div className="mt-1 text-2xl font-semibold">{available ? fmtNum(headroom) : "—"}</div>
      {available ? (
        <>
          <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-white/10">
            <div
              className={pct > 0.85 ? "h-full bg-danger" : pct > 0.6 ? "h-full bg-warning" : "h-full bg-ok"}
              style={{ width: `${Math.min(100, pct * 100)}%` }}
            />
          </div>
          <div className="mt-1 text-xs text-gray-500">
            {fmtNum(used)} / {fmtNum(capacity)} used
          </div>
        </>
      ) : (
        <div className="mt-1 text-xs text-gray-500">redis offline</div>
      )}
    </div>
  );
}

function TopCallersPanel({
  keys,
  onSelectCaller,
}: {
  keys: TopKey[];
  onSelectCaller: (filters: Partial<LogFilters>) => void;
}) {
  return (
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="mb-2 text-sm font-medium text-gray-300">Top callers</div>
      {keys.length === 0 ? (
        <div className="text-sm text-gray-500">No data</div>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-xs text-gray-500">
              <th className="pb-1 font-normal">Caller</th>
              <th className="pb-1 text-right font-normal">Reqs</th>
              <th className="pb-1 text-right font-normal">Err</th>
              <th className="pb-1 text-right font-normal">429</th>
              <th className="pb-1 text-right font-normal">403</th>
            </tr>
          </thead>
          <tbody>
            {keys.map((k) => (
              <tr
                key={k.key_prefix ?? k.discord_id ?? Math.random()}
                className="cursor-pointer border-t border-white/5 hover:bg-white/5"
                onClick={() =>
                  onSelectCaller(
                    k.discord_id ? { discord_id: k.discord_id } : k.key_prefix ? { key_prefix: k.key_prefix } : {},
                  )
                }
              >
                <td className="py-1.5">
                  {k.discord_id ? (
                    <Identity id={k.discord_id} username={k.discord_username} />
                  ) : k.uuid ? (
                    <Identity id={k.uuid} username={k.minecraft_username} kind="minecraft" />
                  ) : (
                    <span className="font-mono text-xs text-gray-400">{k.key_prefix ?? "none"}</span>
                  )}
                </td>
                <td className="text-right">{fmtNum(k.count)}</td>
                <td className="text-right">
                  {k.errors > 0 ? <span className="text-danger">{fmtNum(k.errors)}</span> : <span className="text-gray-600">—</span>}
                </td>
                <td className="text-right">
                  {k.rate_limited > 0 ? (
                    <span className="text-warning">{fmtNum(k.rate_limited)}</span>
                  ) : (
                    <span className="text-gray-600">—</span>
                  )}
                </td>
                <td className="text-right">
                  {k.forbidden > 0 ? (
                    <span className="text-warning">{fmtNum(k.forbidden)}</span>
                  ) : (
                    <span className="text-gray-600">—</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function TopEndpointsPanel({ paths, onSelectPath }: { paths: TopPath[]; onSelectPath: (path: string) => void }) {
  return (
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="mb-2 text-sm font-medium text-gray-300">Top endpoints</div>
      {paths.length === 0 ? (
        <div className="text-sm text-gray-500">No data</div>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-xs text-gray-500">
              <th className="pb-1 font-normal">Path</th>
              <th className="pb-1 text-right font-normal">Reqs</th>
              <th className="pb-1 text-right font-normal">p50/p95/p99</th>
              <th className="pb-1 text-right font-normal">Status</th>
            </tr>
          </thead>
          <tbody>
            {paths.map((p) => (
              <tr
                key={p.path ?? Math.random()}
                className="cursor-pointer border-t border-white/5 hover:bg-white/5"
                onClick={() => p.path && onSelectPath(p.path)}
              >
                <td className="max-w-[220px] truncate py-1.5 font-mono text-xs">{p.path ?? "—"}</td>
                <td className="text-right">{fmtNum(p.count)}</td>
                <td className="text-right font-mono text-xs text-gray-400">
                  {fmtMs(p.p50_ms)} / {fmtMs(p.p95_ms)} / {fmtMs(p.p99_ms)}
                </td>
                <td className="py-1.5 text-right">
                  <div className="ml-auto">
                    <StatusBar
                      counts={[
                        { label: "2xx", count: p.status_2xx, color: STATUS_COLORS.s2xx },
                        { label: "3xx", count: p.status_3xx, color: STATUS_COLORS.s3xx },
                        { label: "4xx", count: p.status_4xx, color: STATUS_COLORS.s4xx },
                        { label: "5xx", count: p.status_5xx, color: STATUS_COLORS.s5xx },
                      ]}
                    />
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

function BudgetsPanel({ rows, onSelectCaller }: { rows: BudgetRow[]; onSelectCaller: (discordId: string) => void }) {
  const active = rows.filter((r) => r.used > 0);
  if (active.length === 0) return null;
  return (
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="mb-2 text-sm font-medium text-gray-300">Keyless auth budget utilization</div>
      <div className="text-xs text-gray-500 mb-2">Session and batch-lookup rate limits, ranked by usage</div>
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-xs text-gray-500">
            <th className="pb-1 font-normal">User</th>
            <th className="pb-1 font-normal">Budget</th>
            <th className="pb-1 text-right font-normal">Used / Limit</th>
            <th className="pb-1 text-right font-normal">Utilization</th>
          </tr>
        </thead>
        <tbody>
          {active.slice(0, 20).map((r) => (
            <tr
              key={`${r.kind}:${r.discord_id}`}
              className="cursor-pointer border-t border-white/5 hover:bg-white/5"
              onClick={() => onSelectCaller(r.discord_id)}
            >
              <td className="py-1.5">
                <Identity id={r.discord_id} username={r.discord_username} />
              </td>
              <td className="text-gray-400">{r.kind === "session" ? "requests / 5min" : "uuids / 5min"}</td>
              <td className="text-right font-mono text-xs">
                {fmtNum(r.used)} / {fmtNum(r.limit)}
              </td>
              <td className={`text-right ${r.utilization > 0.85 ? "text-danger" : r.utilization > 0.6 ? "text-warning" : ""}`}>
                {fmtPercent(r.utilization)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
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
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="text-sm font-medium text-gray-300">Recent requests</div>
        <select
          className="rounded border border-white/10 bg-black/30 px-2 py-1 text-xs"
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
          className="rounded border border-white/10 bg-black/30 px-2 py-1 text-xs"
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
          className="w-28 rounded border border-white/10 bg-black/30 px-2 py-1 text-xs"
          placeholder="status: 429, 4xx…"
          value={draft.status}
          onChange={(e) => setDraft({ ...draft, status: e.target.value })}
          onKeyDown={(e) => e.key === "Enter" && applyDraft()}
        />
        <input
          className="w-48 rounded border border-white/10 bg-black/30 px-2 py-1 text-xs"
          placeholder="caller: name, ID, UUID, or IP"
          value={draft.caller}
          onChange={(e) => setDraft({ ...draft, caller: e.target.value })}
          onKeyDown={(e) => e.key === "Enter" && applyDraft()}
        />
        <input
          className="w-40 rounded border border-white/10 bg-black/30 px-2 py-1 text-xs"
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
        <button onClick={applyDraft} className="rounded border border-white/10 px-3 py-1 text-xs hover:bg-white/10">
          Apply
        </button>
        {activeFilterCount > 0 && (
          <button onClick={onClear} className="text-xs text-gray-500 hover:text-white">
            Clear all ({activeFilterCount})
          </button>
        )}
      </div>
      {chips.length > 0 && (
        <div className="mb-2 flex flex-wrap gap-2">
          {chips.map((c) => (
            <span key={c.key} className="rounded-full bg-white/10 px-2 py-0.5 text-xs">
              {c.label}{" "}
              <button onClick={() => clearChip(c.key)} className="text-gray-400 hover:text-white">
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
      <div className="mt-3 flex items-center justify-between text-xs text-gray-500">
        <span>{total ? `${offset + 1}–${Math.min(offset + LOG_PAGE_SIZE, total)} of ${fmtNum(total)}` : "0 requests"}</span>
        <div className="flex gap-2">
          <button
            disabled={offset === 0}
            onClick={() => onOffsetChange(Math.max(0, offset - LOG_PAGE_SIZE))}
            className="rounded border border-white/10 px-2 py-1 disabled:opacity-40"
          >
            Prev
          </button>
          <button
            disabled={offset + LOG_PAGE_SIZE >= total}
            onClick={() => onOffsetChange(offset + LOG_PAGE_SIZE)}
            className="rounded border border-white/10 px-2 py-1 disabled:opacity-40"
          >
            Next
          </button>
        </div>
      </div>
    </div>
  );
}
