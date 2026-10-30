import { useQuery } from "@tanstack/react-query";
import { RotateCcw } from "lucide-react";
import { useState, type ReactNode } from "react";

import { requestRoutesQuery } from "../../api/requests";
import type { RouteCount } from "../../api/types/request-explorer";
import { CAUSE_LABEL } from "../../kit/badge-styles";
import { Button } from "../../kit/Button";
import { EntityPicker, type EntityPickerValue } from "../../kit/EntityPicker";
import { FIELD_LABEL } from "../../kit/control-styles";
import { Select, type SelectOption } from "../../kit/Select";
import { TimeRangePicker } from "../../kit/TimeRangePicker";
import { formatNumber } from "../../lib/format";
import { normalizeUuid } from "../../lib/uuid";
import { CommitInput } from "./CommitInput";
import {
  CAUSES,
  DEFAULT_WINDOW,
  EMPTY_FILTER,
  describeRequester,
  EXPLORER_WINDOWS,
  KEY_KINDS,
  METHODS,
  parseStatusCode,
  requesterFromRef,
  requesterFromText,
  requesterRef,
  STATUS_CLASSES,
  type ExplorerFilter,
  type RequesterFilter,
} from "./explorer-filter";

type FilterBarProps = {
  filter: ExplorerFilter;
  q: string;
  live: boolean;
  onFilterChange: (filter: ExplorerFilter) => void;
  onQChange: (q: string) => void;
  onLiveChange: (live: boolean) => void;
};

const ANY = "";
const CLASS_LABEL: Record<string, string> = { "2xx": "2xx", "4xx": "4xx", "5xx": "5xx", error: "4xx and 5xx", rate_limited: "429" };
const COUNTRY_CODE = /^[A-Za-z0-9]{2}$/;

export function FilterBar({ filter, q, live, onFilterChange, onQChange, onLiveChange }: FilterBarProps) {
  const routes = useQuery(requestRoutesQuery());
  const set = (patch: Partial<ExplorerFilter>) => onFilterChange({ ...filter, ...patch });
  const relative = filter.range.kind === "window";

  return (
    <section aria-label="Filters" className="flex flex-col gap-3 rounded-md border border-border bg-surface px-3.5 py-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <TimeRangePicker value={filter.range} windows={EXPLORER_WINDOWS} defaultWindow={DEFAULT_WINDOW} onChange={(range) => set({ range })} />
        <div className="flex flex-wrap items-center gap-3">
          <Toggle label="Live" checked={live && relative} disabled={!relative} title={relative ? "Poll for new requests every 5 seconds" : "Live mode needs a window that ends now"} onChange={onLiveChange} />
          <Button
            variant="ghost"
            size="sm"
            icon={<RotateCcw className="size-3" />}
            onClick={() => {
              onQChange("");
              onFilterChange({ ...EMPTY_FILTER, range: filter.range });
            }}
          >
            Clear filters
          </Button>
        </div>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] items-start gap-x-3 gap-y-2.5">
        <Select label="Route" value={filter.route ?? ANY} options={routeOptions(routes.data, filter.route, routes.error)} onChange={(route) => set({ route: route === ANY ? null : route })} />
        <Select label="Method" value={filter.method ?? ANY} options={anyOf("Any method", METHODS.map((method) => ({ value: method, label: method })))} onChange={(method) => set({ method: method === ANY ? null : method })} />
        <Select
          label="Status class"
          value={filter.statusClass ?? ANY}
          options={anyOf("Any status", STATUS_CLASSES.map((statusClass) => ({ value: statusClass, label: CLASS_LABEL[statusClass] })))}
          onChange={(statusClass) => set({ statusClass: STATUS_CLASSES.find((known) => known === statusClass) ?? null })}
        />
        <StatusCodeInput value={filter.status} onChange={(status) => set({ status })} />
        <Select label="Cause" value={filter.cause ?? ANY} options={anyOf("Any cause", CAUSES.map((cause) => ({ value: cause, label: CAUSE_LABEL[cause] })))} onChange={(cause) => set({ cause: CAUSES.find((known) => known === cause) ?? null })} />
        <Select label="Key kind" value={filter.keyKind ?? ANY} options={anyOf("Any key kind", KEY_KINDS.map((kind) => ({ value: kind, label: kind })))} onChange={(kind) => set({ keyKind: KEY_KINDS.find((known) => known === kind) ?? null })} />
        <RequesterPicker value={filter.requester} onChange={(requester) => set({ requester })} />
        <PlayerPicker value={filter.player} onChange={(player) => set({ player, includeDisplay: player === null ? false : filter.includeDisplay })} />
        <CommitInput label="IP" value={filter.ip ?? ""} mono placeholder="203.0.113.9 or 2001:db8::1" onCommit={(ip) => set({ ip: ip === "" ? null : ip })} />
        <CountryInput value={filter.country} onChange={(country) => set({ country })} />
        <CommitInput label="Text" value={q} placeholder="Path, query, error or user agent" onCommit={onQChange} className="w-full" />
      </div>
      {filter.player !== null && (
        <Toggle label="Also match display names (cubelify name=)" checked={filter.includeDisplay} onChange={(includeDisplay) => set({ includeDisplay })} />
      )}
    </section>
  );
}

function RequesterPicker({ value, onChange }: { value: RequesterFilter | null; onChange: (value: RequesterFilter | null) => void }) {
  const [rejected, setRejected] = useState<string | null>(null);
  const pick = (next: EntityPickerValue | null) => {
    const requester = next === null ? null : next.kind === "entity" ? requesterFromRef(next.ref) : requesterFromText(next.raw);
    setRejected(next !== null && requester === null ? (next.kind === "raw" ? next.raw : "that entity") : null);
    if (next === null || requester !== null) onChange(requester);
  };
  return (
    <div className="flex flex-col gap-1">
      <EntityPicker
        label="Requester"
        value={value === null ? null : requesterPickerValue(value)}
        kinds={["account", "player", "key"]}
        allowRaw
        rawHint="Discord id, member id, Minecraft UUID or key prefix"
        placeholder="Account, Minecraft account, key prefix"
        onChange={pick}
      />
      {rejected !== null && <span className="text-11 text-danger">{rejected} is not a Discord id, member id, Minecraft UUID or key prefix.</span>}
    </div>
  );
}

function PlayerPicker({ value, onChange }: { value: string | null; onChange: (value: string | null) => void }) {
  const uuid = value === null ? null : normalizeUuid(value);
  const picked: EntityPickerValue | null = value === null ? null : uuid === null ? { kind: "raw", raw: value } : { kind: "entity", ref: { kind: "player", uuid } };
  return (
    <EntityPicker
      label="Player"
      value={picked}
      kinds={["player", "name"]}
      allowRaw
      rawHint="Any UUID form or a raw name, matched with its name history"
      placeholder="Name or UUID, either form"
      onChange={(next) => onChange(next === null ? null : playerText(next))}
    />
  );
}

function StatusCodeInput({ value, onChange }: { value: number | null; onChange: (value: number | null) => void }) {
  return (
    <ValidatedInput label="Status code" value={value === null ? "" : String(value)} placeholder="429" problem="is not an HTTP status" parse={parseStatusCode} onChange={onChange} />
  );
}

function CountryInput({ value, onChange }: { value: string | null; onChange: (value: string | null) => void }) {
  return <ValidatedInput label="Country" value={value ?? ""} placeholder="NL" problem="is not a two-character country code" parse={countryCode} onChange={onChange} />;
}

function ValidatedInput<T>({ label, value, placeholder, problem, parse, onChange }: { label: string; value: string; placeholder: string; problem: string; parse: (raw: string) => T | null; onChange: (value: T | null) => void }) {
  const [rejected, setRejected] = useState<string | null>(null);
  const commit = (raw: string) => {
    const parsed = raw === "" ? null : parse(raw);
    setRejected(raw !== "" && parsed === null ? raw : null);
    if (raw === "" || parsed !== null) onChange(parsed);
  };
  return <CommitInput label={label} value={value} mono placeholder={placeholder} error={rejected === null ? null : `${rejected} ${problem}.`} onCommit={commit} />;
}

function Toggle({ label, checked, disabled = false, title, onChange }: { label: ReactNode; checked: boolean; disabled?: boolean; title?: string; onChange: (checked: boolean) => void }) {
  return (
    <label title={title} className="inline-flex items-center gap-2 text-12 text-muted has-disabled:opacity-60">
      <input type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} className="size-3.5 accent-accent" />
      <span className={FIELD_LABEL}>{label}</span>
    </label>
  );
}

function requesterPickerValue(requester: RequesterFilter): EntityPickerValue {
  const ref = requesterRef(requester);
  return ref === null ? { kind: "raw", raw: describeRequester(requester) } : { kind: "entity", ref };
}

function countryCode(raw: string): string | null {
  return COUNTRY_CODE.test(raw) ? raw.toUpperCase() : null;
}

function playerText(value: EntityPickerValue): string | null {
  if (value.kind === "raw") return value.raw === "" ? null : (normalizeUuid(value.raw) ?? value.raw);
  if (value.ref.kind === "player") return value.ref.uuid;
  if (value.ref.kind === "name") return value.ref.name;
  return null;
}

function routeOptions(routes: readonly RouteCount[] | undefined, selected: string | null, error: Error | null): SelectOption<string>[] {
  const totals = new Map<string, number>();
  for (const route of routes ?? []) totals.set(route.route, (totals.get(route.route) ?? 0) + route.requests_24h);
  if (selected !== null && !totals.has(selected)) totals.set(selected, 0);
  const anyLabel = error === null ? (routes === undefined ? "Any route (loading routes…)" : "Any route") : `Any route (route list failed: ${error.message})`;
  return anyOf(
    anyLabel,
    [...totals].map(([route, requests]) => ({ value: route, label: `${route} · ${formatNumber(requests)} in 24 h` })),
  );
}

function anyOf(label: string, options: SelectOption<string>[]): SelectOption<string>[] {
  return [{ value: ANY, label }, ...options];
}
