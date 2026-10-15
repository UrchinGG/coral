import { useCallback, useMemo, useState } from "react";

import type { RequestRow } from "../../api/types/requests";
import { useListHotkeys } from "../../app/page-hotkeys";
import { MethodBadge, StatusPill } from "../../kit/Badges";
import { Button } from "../../kit/Button";
import { CopyButton } from "../../kit/Copy";
import { Drawer } from "../../kit/Drawer";
import { EntityLink } from "../../kit/EntityLink";
import { KeyValue } from "../../kit/KeyValue";
import { LazyJsonView } from "../../kit/LazyJsonView";
import { Section } from "../../kit/Section";
import { Table, type Column } from "../../kit/Table";
import { TimeSeriesChart, type ChartRange } from "../../kit/TimeSeriesChart";
import { toast } from "../../kit/toast";
import { formatNumber } from "../../lib/format";
import { buildCurl } from "../../lib/shell";
import { useTimeFormat } from "../../lib/time";
import { CHART_SERIES, CHART_STEP_MS, chartPoints, SAMPLE_KEY, sampleJson, sampleRows } from "./sample-data";

const INFINITE_PAGE_SIZE = 100;
const INFINITE_PAGE_LIMIT = 6;
const INFINITE_PAGE_DELAY_MS = 600;
const VIRTUAL_TABLE_HEIGHT = "420px";
const SAMPLE_CURL = buildCurl({
  method: "POST",
  url: "https://api.urchin.gg/v3/players?note=it's $(not run)",
  headers: [
    ["x-api-key", SAMPLE_KEY],
    ["user-agent", "ExampleClient/1.0\ninjected: no"],
    ["bad header", "omitted"],
  ],
  body: '{"uuids":["3f9a1c2e7b4d4e8f9a6b1c3d5e7f9012"]}',
});

export function DataDemos() {
  return (
    <>
      <ChartDemo />
      <div className="grid gap-4 xl:grid-cols-2">
        <VirtualTableDemo />
        <InfiniteTableDemo />
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <Section title="JSON view" count="lazy chunk · pages of 200 · search · per-node copy">
          <LazyJsonView value={sampleJson()} label="Sample body" links={{ players: true, accounts: true }} />
        </Section>
        <Section title="Copy curl" count="POSIX-quoted, body through stdin">
          <div className="flex flex-col gap-2">
            <pre className="m-0 max-h-[320px] overflow-auto rounded-md bg-bg px-3.5 py-3 font-mono text-12 leading-[1.6] break-all whitespace-pre-wrap">{SAMPLE_CURL.command}</pre>
            <div className="flex flex-wrap items-center gap-2 text-12 text-muted">
              <CopyButton text={SAMPLE_CURL.command}>Copy curl</CopyButton>
              {SAMPLE_CURL.omittedHeaders.length > 0 && <span>Omitted headers a shell cannot carry: {SAMPLE_CURL.omittedHeaders.join(", ")}</span>}
            </div>
          </div>
        </Section>
      </div>
    </>
  );
}

function ChartDemo() {
  const { time } = useTimeFormat();
  const [now] = useState(() => Date.now());
  const points = useMemo(() => chartPoints(now), [now]);
  const [brush, setBrush] = useState<ChartRange | null>(null);
  return (
    <Section
      title="Time series with brush"
      count={brush === null ? "drag across the chart to select" : `${time(brush.from, { seconds: false })} → ${time(brush.to, { seconds: false })}`}
      actions={
        <Button size="sm" variant="ghost" onClick={() => setBrush(null)} disabled={brush === null}>
          Select whole window
        </Button>
      }
    >
      <TimeSeriesChart
        label="Requests per minute by status class, stacked, with p95 latency below"
        points={points}
        stepMs={CHART_STEP_MS}
        series={CHART_SERIES}
        unit="req/min"
        line={{ label: "p95 latency", unit: "ms", color: "var(--color-chart-line)" }}
        brush={brush}
        onBrush={setBrush}
        onPointClick={(t) => toast.ok(`Clicked ${time(t, { zone: true })}; a page would open Requests here.`)}
        formatTick={(t) => time(t, { seconds: false, date: "never" })}
        formatTooltipTime={(t) => time(t, { seconds: false, zone: true })}
      />
    </Section>
  );
}

function useRequestColumns(): readonly Column<RequestRow>[] {
  const { time } = useTimeFormat();
  return useMemo(
    () => [
      { id: "time", header: "Time", cell: (row) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(row.ts, { millis: true, date: "never" })}</span> },
      {
        id: "route",
        header: "Route",
        cell: (row) => (
          <span className="inline-flex items-center gap-1.5">
            <MethodBadge method={row.method} />
            <span className="font-mono text-[11.5px]">{row.route}</span>
          </span>
        ),
      },
      { id: "requester", header: "Requester", cell: (row) => <EntityLink entity={row.requester} />, priority: 2 },
      { id: "status", header: "Status", cell: (row) => <StatusPill status={row.status} outcomeStatus={row.outcome_status} cause={row.cause} /> },
      { id: "latency", header: "Latency", cell: (row) => `${formatNumber(row.latency_ms)} ms`, numeric: true, priority: 2 },
      { id: "ip", header: "IP", cell: (row) => (row.ip === null ? "—" : <EntityLink entity={row.ip} />), priority: 3 },
    ],
    [time],
  );
}

function VirtualTableDemo() {
  const [now] = useState(() => Date.now());
  const rows = useMemo(() => sampleRows(now), [now]);
  const columns = useRequestColumns();
  const [selected, setSelected] = useState<string | null>(null);
  const rowKey = useCallback((row: RequestRow) => row.request_id, []);
  const selectedRow = rows.find((row) => row.request_id === selected) ?? null;
  useListHotkeys({ rows, rowKey, selectedKey: selected, onSelect: setSelected, onOpen: (row) => toast.ok(`Would open ${row.request_id}`) });
  return (
    <Section title="Virtual table" count={`${formatNumber(rows.length)} rows · j/k, Enter, o, Esc`} flush>
      <Table label="Sample requests" columns={columns} rows={rows} rowKey={rowKey} selectedKey={selected} onRowClick={(row) => setSelected(row.request_id)} virtual height={VIRTUAL_TABLE_HEIGHT} />
      <Drawer open={selectedRow !== null} onClose={() => setSelected(null)} title="Request preview">
        {selectedRow !== null && (
          <KeyValue
            items={[
              { label: "Request", value: <EntityLink entity={{ kind: "request", request_id: selectedRow.request_id, ts: selectedRow.ts }} /> },
              { label: "Status", value: <StatusPill status={selectedRow.status} outcomeStatus={selectedRow.outcome_status} cause={selectedRow.cause} /> },
              { label: "Path", value: <span className="font-mono">{selectedRow.path}</span> },
              { label: "Query", value: <span className="font-mono">{selectedRow.query ?? "—"}</span> },
              { label: "Requester", value: <EntityLink entity={selectedRow.requester} /> },
            ]}
          />
        )}
      </Drawer>
    </Section>
  );
}

function InfiniteTableDemo() {
  const [now] = useState(() => Date.now());
  const [pages, setPages] = useState(1);
  const [loading, setLoading] = useState(false);
  const rows = useMemo(() => sampleRows(now, pages * INFINITE_PAGE_SIZE), [now, pages]);
  const columns = useRequestColumns();
  const loadMore = useCallback(() => {
    setLoading(true);
    window.setTimeout(() => {
      setPages((count) => count + 1);
      setLoading(false);
    }, INFINITE_PAGE_DELAY_MS);
  }, []);
  return (
    <Section title="Infinite table" count={`${formatNumber(rows.length)} rows loaded · next page within two screens of the end`} flush fetching={loading}>
      <Table
        label="Sample requests, paged"
        columns={columns}
        rows={rows}
        rowKey={(row) => row.request_id}
        virtual
        height={VIRTUAL_TABLE_HEIGHT}
        infinite={{ hasMore: pages < INFINITE_PAGE_LIMIT, loading, onLoadMore: loadMore }}
        footer="scanned back to the start of the sample"
      />
    </Section>
  );
}
