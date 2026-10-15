import { ChevronDown, ChevronRight } from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";

import type { EntityRef } from "../api/types/refs";
import { useDebouncedValue } from "../lib/debounce";
import { formatNumber, plural } from "../lib/format";
import { isSnowflake } from "../lib/refs";
import { isUndashedUuid, isUuidV7 } from "../lib/uuid";
import { CopyButton } from "./Copy";
import { EntityLink } from "./EntityLink";
import { Input } from "./Input";
import { Segmented } from "./Segmented";

export type JsonLinks = { players?: boolean; accounts?: boolean };

type JsonViewProps = {
  value: unknown;
  raw?: string;
  initialDepth?: number;
  label?: string;
  links?: JsonLinks;
};

type JsonContainer = readonly unknown[] | Record<string, unknown>;

type SearchResult = {
  matched: ReadonlySet<string>;
  ancestors: ReadonlySet<string>;
  required: ReadonlyMap<string, number>;
  count: number;
  capped: boolean;
};

type TreeContext = {
  initialDepth: number;
  needle: string;
  links: JsonLinks;
  search: SearchResult;
  isOpen: (path: string, depth: number) => boolean;
  toggle: (path: string, depth: number) => void;
  visibleCount: (path: string) => number;
  showMore: (path: string) => void;
};

type NodeProps = {
  name: string | null;
  value: unknown;
  path: string;
  depth: number;
  trailingComma: boolean;
  context: TreeContext;
};

type ContainerProps = NodeProps & { value: JsonContainer };

type ViewMode = "tree" | "raw";

const PAGE_SIZE = 200;
const DEFAULT_DEPTH = 1;
const INDENT_PX = 14;
const SEARCH_DEBOUNCE_MS = 150;
const MAX_MATCHES = 500;
const ROOT_PATH = "$";
const MODES = [
  { value: "tree", label: "Tree" },
  { value: "raw", label: "Raw" },
] as const;
const EMPTY_SEARCH: SearchResult = { matched: new Set(), ancestors: new Set(), required: new Map(), count: 0, capped: false };
const NO_LINKS: JsonLinks = {};
const BRACKETS = { array: ["[", "]"], object: ["{", "}"] } as const;

export function JsonView({ value, raw, initialDepth = DEFAULT_DEPTH, label = "JSON", links = NO_LINKS }: JsonViewProps) {
  const [mode, setMode] = useState<ViewMode>("tree");
  const [query, setQuery] = useState("");
  const needle = useDebouncedValue(query.trim().toLowerCase(), SEARCH_DEBOUNCE_MS);
  const pretty = useMemo(() => JSON.stringify(value, null, 2), [value]);
  const search = useMemo(() => (needle === "" ? EMPTY_SEARCH : searchTree(value, needle)), [value, needle]);

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <Segmented label={`${label} view`} value={mode} options={MODES} onChange={setMode} />
        {mode === "tree" && (
          <>
            <Input aria-label={`Search ${label}`} placeholder="Search keys and values" value={query} onChange={(event) => setQuery(event.target.value)} className="w-56" />
            {needle !== "" && (
              <span className="text-11 text-subtle">
                {search.capped ? `${formatNumber(MAX_MATCHES)}+ matches` : plural(search.count, "match", "matches")}
              </span>
            )}
          </>
        )}
        <span className="ml-auto">
          <CopyButton text={mode === "raw" ? (raw ?? pretty) : pretty} label={`Copy ${label}`}>
            Copy
          </CopyButton>
        </span>
      </div>
      {mode === "raw" ? (
        <pre className="m-0 max-h-[480px] overflow-auto rounded-md bg-bg px-3.5 py-3 font-mono text-12 leading-[1.6] break-all whitespace-pre-wrap">{raw ?? pretty}</pre>
      ) : (
        <JsonTree key={needle} value={value} initialDepth={initialDepth} needle={needle} search={search} links={links} />
      )}
    </div>
  );
}

function JsonTree({ value, initialDepth, needle, search, links }: { value: unknown; initialDepth: number; needle: string; search: SearchResult; links: JsonLinks }) {
  const [toggled, setToggled] = useState<ReadonlyMap<string, boolean>>(new Map());
  const [shown, setShown] = useState<ReadonlyMap<string, number>>(new Map());

  const isOpen = (path: string, depth: number) => toggled.get(path) ?? (search.ancestors.has(path) || depth < initialDepth);
  const context: TreeContext = {
    initialDepth,
    needle,
    links,
    search,
    isOpen,
    toggle: (path, depth) => setToggled(new Map(toggled).set(path, !isOpen(path, depth))),
    visibleCount: (path) => shown.get(path) ?? Math.max(PAGE_SIZE, roundUpToPage(search.required.get(path) ?? 0)),
    showMore: (path) => setShown(new Map(shown).set(path, context.visibleCount(path) + PAGE_SIZE)),
  };

  return (
    <div className="max-h-[480px] overflow-auto rounded-md bg-bg px-2 py-2 font-mono text-12 leading-[1.6]">
      <JsonNode name={null} value={value} path={ROOT_PATH} depth={0} trailingComma={false} context={context} />
    </div>
  );
}

function JsonNode(props: NodeProps) {
  return isContainer(props.value) ? <ContainerNode {...props} value={props.value} /> : <LeafNode {...props} />;
}

function ContainerNode(props: ContainerProps) {
  const { name, value, path, depth, trailingComma, context } = props;
  const [open, close] = bracketsOf(value);
  const entries = entriesOf(value);
  if (entries.length === 0) {
    return (
      <Row depth={depth} copy={value}>
        <KeyLabel name={name} needle={context.needle} />
        <span className="text-subtle">
          {open}
          {close}
          {trailingComma && ","}
        </span>
      </Row>
    );
  }
  const expanded = context.isOpen(path, depth);
  return (
    <>
      <ContainerHeader {...props} count={entries.length} expanded={expanded} />
      {expanded && <ContainerChildren {...props} entries={entries} />}
    </>
  );
}

function ContainerHeader({ name, value, path, depth, trailingComma, context, count, expanded }: ContainerProps & { count: number; expanded: boolean }) {
  const [open, close] = bracketsOf(value);
  const Chevron = expanded ? ChevronDown : ChevronRight;
  return (
    <Row depth={depth} copy={value}>
      <button
        type="button"
        aria-expanded={expanded}
        aria-label={expanded ? "Collapse" : "Expand"}
        onClick={() => context.toggle(path, depth)}
        className="-ml-4 inline-flex size-4 shrink-0 items-center justify-center self-center text-subtle hover:text-fg"
      >
        <Chevron className="size-3" />
      </button>
      <KeyLabel name={name} needle={context.needle} />
      <span className="text-subtle">{open}</span>
      {!expanded && (
        <span className="text-subtle">
          … {close}
          {trailingComma && ","} <span className="font-sans text-11">{Array.isArray(value) ? plural(count, "item") : plural(count, "key")}</span>
        </span>
      )}
    </Row>
  );
}

function ContainerChildren({ value, path, depth, trailingComma, context, entries }: ContainerProps & { entries: readonly [string, unknown][] }) {
  const array = Array.isArray(value);
  const [, close] = bracketsOf(value);
  const visible = context.visibleCount(path);
  return (
    <>
      {entries.slice(0, visible).map(([key, child], index) => (
        <JsonNode key={key} name={array ? null : key} value={child} path={childPath(path, key)} depth={depth + 1} trailingComma={index < entries.length - 1} context={context} />
      ))}
      {entries.length > visible && (
        <div style={{ paddingLeft: (depth + 1) * INDENT_PX }}>
          <button type="button" onClick={() => context.showMore(path)} className="font-sans text-11 text-accent">
            Show next {formatNumber(Math.min(PAGE_SIZE, entries.length - visible))} of {formatNumber(entries.length - visible)} remaining
          </button>
        </div>
      )}
      <Row depth={depth}>
        <span className="text-subtle">
          {close}
          {trailingComma && ","}
        </span>
      </Row>
    </>
  );
}

function LeafNode({ name, value, path, depth, trailingComma, context }: NodeProps) {
  return (
    <Row depth={depth} copy={value} highlighted={context.search.matched.has(path)}>
      <KeyLabel name={name} needle={context.needle} />
      <span className="min-w-0 break-all">
        <LeafValue value={value} needle={context.needle} links={context.links} />
        {trailingComma && <span className="text-subtle">,</span>}
      </span>
    </Row>
  );
}

function LeafValue({ value, needle, links }: { value: unknown; needle: string; links: JsonLinks }) {
  if (typeof value !== "string") return <span className="text-warning">{highlight(String(value), needle)}</span>;
  const entity = linkedEntity(value, links);
  if (entity === null) return <span className="text-json-string">"{highlight(value, needle)}"</span>;
  return (
    <span className="text-json-string">
      "<EntityLink entity={entity} label={value} mono plain eagerLabel={false} />"
    </span>
  );
}

function KeyLabel({ name, needle }: { name: string | null; needle: string }) {
  if (name === null) return null;
  return (
    <span className="shrink-0">
      <span className="text-json-key">"{highlight(name, needle)}"</span>
      <span className="text-subtle">: </span>
    </span>
  );
}

function Row({ depth, copy, highlighted = false, children }: { depth: number; copy?: unknown; highlighted?: boolean; children: ReactNode }) {
  return (
    <div
      className={`group/json flex items-start gap-0.5 rounded-sm pr-1 ${highlighted ? "bg-warning/8" : "hover:bg-white/[0.03]"}`}
      style={{ paddingLeft: depth * INDENT_PX + INDENT_PX }}
    >
      <div className="flex min-w-0 flex-1 flex-wrap items-start">{children}</div>
      {copy !== undefined && (
        <span className="opacity-0 group-hover/json:opacity-100 focus-within:opacity-100">
          <CopyButton text={() => copyTextOf(copy)} label="Copy value" />
        </span>
      )}
    </div>
  );
}

function highlight(text: string, needle: string): ReactNode {
  if (needle === "") return text;
  const lower = text.toLowerCase();
  const parts: ReactNode[] = [];
  let cursor = 0;
  let found = lower.indexOf(needle);
  while (found >= 0) {
    parts.push(text.slice(cursor, found), <mark key={found}>{text.slice(found, found + needle.length)}</mark>);
    cursor = found + needle.length;
    found = lower.indexOf(needle, cursor);
  }
  parts.push(text.slice(cursor));
  return parts;
}

function searchTree(root: unknown, needle: string): SearchResult {
  const matched = new Set<string>();
  const ancestors = new Set<string>();
  const required = new Map<string, number>();
  const trail: { path: string; index: number }[] = [];

  const record = (path: string) => {
    matched.add(path);
    for (const step of trail) {
      ancestors.add(step.path);
      required.set(step.path, Math.max(required.get(step.path) ?? 0, step.index + 1));
    }
  };

  const visit = (value: unknown, path: string, key: string | null) => {
    if (matched.size >= MAX_MATCHES) return;
    const keyHit = key !== null && key.toLowerCase().includes(needle);
    const valueHit = !isContainer(value) && String(value).toLowerCase().includes(needle);
    if (keyHit || valueHit) record(path);
    if (!isContainer(value)) return;
    const array = Array.isArray(value);
    entriesOf(value).forEach(([childKey, child], index) => {
      trail.push({ path, index });
      visit(child, childPath(path, childKey), array ? null : childKey);
      trail.pop();
    });
  };

  visit(root, ROOT_PATH, null);
  return { matched, ancestors, required, count: matched.size, capped: matched.size >= MAX_MATCHES };
}

function linkedEntity(value: string, links: JsonLinks): EntityRef | null {
  if (links.players === true && isUndashedUuid(value) && !isUuidV7(value)) return { kind: "player", uuid: value.toLowerCase() };
  if (links.accounts === true && isSnowflake(value)) return { kind: "account", discord_id: value, member_id: null };
  return null;
}

function isContainer(value: unknown): value is JsonContainer {
  return typeof value === "object" && value !== null;
}

function bracketsOf(value: JsonContainer): readonly [string, string] {
  return Array.isArray(value) ? BRACKETS.array : BRACKETS.object;
}

function entriesOf(value: JsonContainer): [string, unknown][] {
  return Array.isArray(value) ? value.map((child, index) => [String(index), child]) : Object.entries(value);
}

function childPath(parent: string, key: string): string {
  return `${parent}[${JSON.stringify(key)}]`;
}

function roundUpToPage(count: number): number {
  return Math.ceil(count / PAGE_SIZE) * PAGE_SIZE;
}

function copyTextOf(value: unknown): string {
  return typeof value === "string" ? value : JSON.stringify(value, null, 2);
}
