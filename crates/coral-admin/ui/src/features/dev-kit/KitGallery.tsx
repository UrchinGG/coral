import { Info, Trash2 } from "lucide-react";
import { lazy, Suspense, useState } from "react";

import { ApiError } from "../../api/http";
import { useSession } from "../../app/boot";
import { Badge, Kbd, MethodBadge, Pill, StatusPill } from "../../kit/Badges";
import { Button, IconButton } from "../../kit/Button";
import { ClampText } from "../../kit/ClampText";
import { Combobox, type ComboboxOption } from "../../kit/Combobox";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { CopyButton, CopyField } from "../../kit/Copy";
import { CoverageNotices } from "../../kit/CoverageNotices";
import { EntityLink } from "../../kit/EntityLink";
import { EntityPicker, type EntityPickerValue } from "../../kit/EntityPicker";
import { HoverCard, Tooltip } from "../../kit/HoverCard";
import { Input, Textarea } from "../../kit/Input";
import { KeyValue } from "../../kit/KeyValue";
import { KpiStrip, KpiTile } from "../../kit/KpiTile";
import { Meter } from "../../kit/Meter";
import { Section } from "../../kit/Section";
import { Segmented } from "../../kit/Segmented";
import { Select } from "../../kit/Select";
import { Sparkline } from "../../kit/Sparkline";
import { CoverageBanner, EmptyState, ErrorState, Skeleton, SkeletonLines, SkeletonRows } from "../../kit/States";
import { Tabs } from "../../kit/Tabs";
import { Timeline } from "../../kit/Timeline";
import { TimeRangePicker, type TimeRange } from "../../kit/TimeRangePicker";
import { TONE_SOFT, type Tone } from "../../kit/tone";
import { refIdentity } from "../../lib/refs";
import { OverlayDemos } from "./OverlayDemos";
import { SAMPLE_ACCOUNT, SAMPLE_KEY, SAMPLE_PLAYER, SAMPLE_REFS, SAMPLE_TIMELINE } from "./sample-data";

const DataDemos = lazy(() => import("./DataDemos").then((module) => ({ default: module.DataDemos })));

type GalleryWindow = "1h" | "6h" | "24h" | "7d";
type GalleryTab = "activity" | "network" | "lookups";

const WINDOWS: readonly GalleryWindow[] = ["1h", "6h", "24h", "7d"];
const TONES: readonly Tone[] = ["neutral", "ok", "info", "warning", "limited", "upstream", "danger", "accent"];
const SPARK = [12, 18, 15, 22, 30, 26, 34, 31, 40, 38, 44, 41];
const DAY_MS = 86_400_000;
const COVERAGE_DEMO_DAYS = 14;
const SAMPLE_ERROR = new ApiError({
  status: 503,
  code: "unavailable",
  message: "The traffic rollup has not run yet",
  endpoint: "GET /api/traffic/summary?window=1h",
  serverMs: 4.2,
  serverTime: "Wed, 14 Oct 2026 14:32:10 GMT",
});
const ROUTE_OPTIONS: readonly ComboboxOption<string>[] = ["/v3/players", "/v3/player/tags", "/v3/player/profile", "/v3/cubelify", "/v3/resolve/{identifier}"].map(
  (route) => ({ key: route, value: route, text: route }),
);

export function KitGallery() {
  return (
    <>
      <div className="flex flex-col gap-1">
        <h1 className="text-22 font-semibold">Component kit</h1>
        <p className="m-0 max-w-[70ch] text-muted">Every kit component with invented sample data. Entity links on this page request labels for their sample ids.</p>
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <ButtonsDemo />
        <BadgesDemo />
      </div>
      <EntityLinksDemo />
      <InputsDemo />
      <div className="grid gap-4 lg:grid-cols-2">
        <DetailsDemo />
        <StatesDemo />
      </div>
      <MetricsDemo />
      <Suspense fallback={<SkeletonRows />}>
        <DataDemos />
      </Suspense>
      <OverlayDemos />
    </>
  );
}

function ButtonsDemo() {
  const [pending, setPending] = useState(false);
  return (
    <Section title="Buttons">
      <div className="flex flex-wrap items-center gap-2">
        <Button>Default</Button>
        <Button variant="primary">Primary</Button>
        <Button variant="danger" icon={<Trash2 className="size-3" />}>
          Danger
        </Button>
        <Button variant="ghost">Ghost</Button>
        <Button size="sm">Small</Button>
        <Button pending={pending} onClick={() => setPending(!pending)}>
          {pending ? "Pending (click to stop)" : "Toggle pending"}
        </Button>
        <Button disabled>Disabled</Button>
        <IconButton label="Information">
          <Info className="size-3.5" />
        </IconButton>
        <ConfirmButton variant="danger" onConfirm={() => setPending(false)}>
          Reset limits
        </ConfirmButton>
      </div>
    </Section>
  );
}

function BadgesDemo() {
  return (
    <Section title="Pills, badges and keys">
      <div className="flex flex-col gap-2.5">
        <div className="flex flex-wrap gap-1.5">
          {TONES.map((tone) => (
            <Pill key={tone} tone={tone}>
              {tone}
            </Pill>
          ))}
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          <Badge>neutral</Badge>
          <Badge variant="cf">CF Worker</Badge>
          <Badge variant="byname">by name</Badge>
          <Badge variant="bot">coral-bot</Badge>
          <Badge variant="key">dev key</Badge>
          <Badge variant="danger">locked</Badge>
          {["GET", "POST", "PATCH", "DELETE", "OPTIONS"].map((method) => (
            <MethodBadge key={method} method={method} />
          ))}
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <StatusPill status={200} outcomeStatus={null} cause="ok" />
          <StatusPill status={429} outcomeStatus={null} cause="caller_limit" />
          <StatusPill status={503} outcomeStatus={null} cause="upstream_limit" />
          <StatusPill status={404} outcomeStatus={200} cause="not_found" />
          <StatusPill status={500} outcomeStatus={null} cause="internal" />
        </div>
        <div className="flex flex-wrap items-center gap-1.5 text-12 text-muted">
          <Kbd>⌘K</Kbd> <Kbd>g h</Kbd> <Kbd>j</Kbd> <Kbd>?</Kbd>
          {(["ok", "warning", "danger"] as const).map((tone) => (
            <span key={tone} className={`rounded border px-1.5 ${TONE_SOFT[tone]}`}>
              {tone} surface
            </span>
          ))}
        </div>
      </div>
    </Section>
  );
}

function EntityLinksDemo() {
  return (
    <Section title="Entity links" count="hover for 350 ms for the card; focus prefetches">
      <div className="flex flex-wrap gap-x-5 gap-y-2.5 text-12">
        {SAMPLE_REFS.map((ref) => (
          <span key={refIdentity(ref)} className="inline-flex items-center gap-1.5">
            <span className="text-11 text-subtle">{ref.kind}</span>
            <EntityLink entity={ref} />
          </span>
        ))}
        <span className="inline-flex items-center gap-1.5">
          <span className="text-11 text-subtle">with tab</span>
          <EntityLink entity={{ kind: "account", discord_id: SAMPLE_ACCOUNT, member_id: null }} tab="lookups" label="Bot lookups" />
        </span>
      </div>
    </Section>
  );
}

function InputsDemo() {
  const [text, setText] = useState("");
  const [windowValue, setWindowValue] = useState<GalleryWindow>("24h");
  const [tab, setTab] = useState<GalleryTab>("activity");
  const [range, setRange] = useState<TimeRange<GalleryWindow>>({ kind: "window", window: "24h" });
  const [route, setRoute] = useState("");
  const [picked, setPicked] = useState<EntityPickerValue | null>(null);
  return (
    <Section title="Inputs">
      <div className="flex flex-col gap-4">
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Input label="Input" placeholder="Type here" value={text} onChange={(event) => setText(event.target.value)} />
          <Input label="Mono input" mono placeholder="94589c87…" hint="Mono for ids" />
          <Select label="Select" value={windowValue} options={WINDOWS.map((value) => ({ value, label: value }))} onChange={setWindowValue} />
          <Combobox label="Combobox" placeholder="Filter routes" options={ROUTE_OPTIONS} query={route} onQueryChange={setRoute} onSelect={(option) => setRoute(option.value)} mono />
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <EntityPicker
            label="Player (picked or raw)"
            value={picked}
            onChange={setPicked}
            kinds={["player", "name"]}
            operator="p:"
            allowRaw
            rawHint="Matches every request naming this UUID or name"
            placeholder="Name or UUID"
          />
          <Textarea label="Textarea" placeholder="Reason shown to the member" />
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <Segmented label="Window" value={windowValue} options={WINDOWS.map((value) => ({ value, label: value }))} onChange={setWindowValue} />
          <TimeRangePicker value={range} windows={WINDOWS} defaultWindow="24h" onChange={setRange} />
        </div>
        <Tabs
          label="Account sections"
          value={tab}
          onChange={setTab}
          tabs={[
            { value: "activity", label: "Activity" },
            { value: "network", label: "Network", count: 14 },
            { value: "lookups", label: "Lookups", count: 52_118 },
          ]}
        />
      </div>
    </Section>
  );
}

function DetailsDemo() {
  return (
    <Section title="Key-value, copy, clamp, timeline" flush>
      <div className="flex flex-col gap-3 px-3.5 py-3">
        <KeyValue
          items={[
            { label: "Account", value: <EntityLink entity={{ kind: "account", discord_id: SAMPLE_ACCOUNT, member_id: 23025 }} /> },
            { label: "Player", value: <CopyField value={SAMPLE_PLAYER} label="Copy UUID" /> },
            { label: "API key", value: <CopyField value={SAMPLE_KEY} label="Copy key" secret /> },
            {
              label: "Note",
              value: (
                <ClampText>
                  A long free-text value wraps and clamps to two lines in dense rows. The in-place control reveals the rest without leaving the row, and
                  nothing is ever cut off for good: the whole text is one click away, including identifiers such as {SAMPLE_PLAYER}.
                </ClampText>
              ),
            },
            {
              label: "Copy",
              value: (
                <span className="inline-flex items-center gap-2">
                  <CopyButton text={SAMPLE_PLAYER} label="Copy UUID" />
                  <CopyButton text={SAMPLE_PLAYER}>Copy UUID</CopyButton>
                </span>
              ),
            },
          ]}
        />
      </div>
      <div className="border-t border-border">
        <Timeline items={SAMPLE_TIMELINE} />
      </div>
    </Section>
  );
}

function StatesDemo() {
  const session = useSession();
  const now = Date.now();
  return (
    <Section title="Loading, empty, error and coverage" fetching>
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-3">
          <Skeleton className="size-11 rounded-lg" />
          <div className="flex-1">
            <SkeletonLines />
          </div>
        </div>
        <div className="rounded-md border border-border">
          <SkeletonRows count={3} />
        </div>
        <EmptyState action={<Button size="sm">Clear filters</Button>}>No requests match these filters in the last 24 h.</EmptyState>
        <ErrorState error={SAMPLE_ERROR} onRetry={() => undefined} />
        <CoverageBanner>Batch-body lookups before 2026-10-14 03:14 UTC were never captured (412 batch requests in this window).</CoverageBanner>
        <CoverageBanner tone="warning">Rollups are behind by 9 minutes; the last bucket is partial.</CoverageBanner>
        <CoverageNotices
          coverage={session.coverage}
          features={["detail", "attribution", "subjects", "body_subjects", "rollups"]}
          from={new Date(now - COVERAGE_DEMO_DAYS * DAY_MS).toISOString()}
          to={new Date(now).toISOString()}
        />
      </div>
    </Section>
  );
}

function MetricsDemo() {
  return (
    <Section title="KPIs, sparkline, meters and hover">
      <div className="flex flex-col gap-4">
        <KpiStrip>
          <KpiTile label="Requests" value="1,204,331" sub={<span className="text-danger">▲ 12% vs previous 24h</span>} />
          <KpiTile label="429 caller" swatch="#fb923c" value="8,214" sub="0.68%" />
          <KpiTile label="p50 / p95" value="38" unit="/ 412 ms" sub="in selection" />
          <KpiTile label="Pulse" value="412" unit="req/min">
            <Sparkline values={SPARK} color="#818cf8" label="Requests per minute, last 12 minutes" />
          </KpiTile>
        </KpiStrip>
        <div className="grid gap-3 sm:grid-cols-3">
          <Meter used={112} limit={300} label="personal / 5 min" />
          <Meter used={252} limit={300} label="keyuuids / 5 min" />
          <Meter used={297} limit={300} label="capacity" unit="per 5 min" />
        </div>
        <div className="flex flex-wrap items-center gap-4 text-12">
          <Meter inline used={41} limit={60} />
          <Tooltip text="Tooltips appear on hover and on keyboard focus">
            <Button size="sm">Tooltip</Button>
          </Tooltip>
          <HoverCard content={() => <span>A hover card holds richer content and stays open while the pointer is over it.</span>}>
            <span className="text-accent underline decoration-dotted">Hover card</span>
          </HoverCard>
        </div>
      </div>
    </Section>
  );
}
