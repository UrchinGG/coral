import { ArrowRight, Search } from "lucide-react";
import type { ReactNode } from "react";

import { useLabel } from "../api/labels";
import type { EntityRef } from "../api/types/refs";
import { Kbd } from "../kit/Badges";
import { ENTITY_ICON, ENTITY_TEXT } from "../kit/entity-style";
import { cx } from "../lib/cx";
import { ENTITY_KIND_LABEL, labelTargetOf, refFallbackText } from "../lib/refs";
import { failureSummary, type OmniboxItem } from "./results";
import type { OmniboxStatus } from "./status";
import type { OmniboxResults } from "./useOmnibox";

type OmniboxListProps = {
  results: OmniboxResults;
  active: number;
  listId: string;
  onHover: (index: number) => void;
  onChoose: (item: OmniboxItem) => void;
};

export function OmniboxList({ results, active, listId, onHover, onChoose }: OmniboxListProps) {
  let index = 0;
  return (
    <div className="flex min-h-0 flex-col">
      <StatusLine status={results.status} />
      <FailureLine summary={failureSummary(results.failures)} />
      {results.remoteError !== null && <WarningLine>Mojang lookup failed: {results.remoteError.message}. Results below are local only.</WarningLine>}
      <div id={listId} role="listbox" aria-label="Results" className="min-h-0 overflow-y-auto p-1.5">
        {results.sections
          .filter((section) => section.items.length > 0)
          .map((section) => (
            <div key={section.title} role="group" aria-label={section.title}>
              <div className="px-2.5 pt-2 pb-1 text-11 font-semibold tracking-[0.07em] text-subtle uppercase">{section.title}</div>
              {section.items.map((item) => {
                const itemIndex = index++;
                return (
                  <OmniboxRow
                    key={item.key}
                    id={`${listId}-${itemIndex}`}
                    item={item}
                    selected={itemIndex === active}
                    onHover={() => onHover(itemIndex)}
                    onChoose={() => onChoose(item)}
                  />
                );
              })}
            </div>
          ))}
        {results.items.length === 0 && <div className="px-3.5 py-[18px] text-12 text-muted">Nothing to show yet. Type a player, UUID, Discord id or name, IP, key prefix or request id.</div>}
      </div>
    </div>
  );
}

export function PaletteFooter() {
  return (
    <div className="flex flex-wrap gap-x-3.5 gap-y-1.5 border-t border-border-strong px-3.5 py-2 text-11 text-subtle">
      <span>
        <Kbd>↑</Kbd> <Kbd>↓</Kbd> move
      </span>
      <span>
        <Kbd>↵</Kbd> open
      </span>
      <span>
        Force a type: <code className="font-mono text-muted">p:</code> player · <code className="font-mono text-muted">a:</code> account ·{" "}
        <code className="font-mono text-muted">k:</code> key · <code className="font-mono text-muted">ip:</code> · <code className="font-mono text-muted">r:</code> request ·{" "}
        <code className="font-mono text-muted">g:</code> guild · <code className="font-mono text-muted">pl:</code> plugin · <code className="font-mono text-muted">n:</code> raw name
      </span>
    </div>
  );
}

function OmniboxRow({ id, item, selected, onHover, onChoose }: { id: string; item: OmniboxItem; selected: boolean; onHover: () => void; onChoose: () => void }) {
  return (
    <div
      id={id}
      role="option"
      aria-selected={selected}
      onPointerMove={selected ? undefined : onHover}
      onPointerDown={(event) => event.preventDefault()}
      onClick={onChoose}
      className={cx(
        "grid cursor-pointer grid-cols-[28px_minmax(0,1fr)_auto] items-center gap-2.5 rounded-md px-2.5 py-[7px]",
        selected && "bg-accent/14 shadow-[inset_0_0_0_1px_rgba(129,140,248,0.35)]",
      )}
    >
      {item.type === "entity" ? <EntityRowBody entity={item.ref} detail={item.detail} /> : <LinkRowBody item={item} />}
      <span className="text-right text-11 whitespace-nowrap text-subtle">
        {item.type === "entity" ? ENTITY_KIND_LABEL[item.ref.kind] : linkType(item)}
        {selected && (
          <span className="ml-1.5">
            <Kbd>↵</Kbd>
          </span>
        )}
      </span>
    </div>
  );
}

function EntityRowBody({ entity, detail }: { entity: EntityRef; detail: string }) {
  const name = useLabel(labelTargetOf(entity));
  const fallback = refFallbackText(entity);
  const Icon = ENTITY_ICON[entity.kind];
  return (
    <>
      <span className={cx("flex size-7 items-center justify-center rounded-md bg-surface-2", ENTITY_TEXT[entity.kind])}>
        <Icon className="size-3.5" />
      </span>
      <span className="min-w-0">
        <span className={cx("block font-medium wrap-anywhere", name === null && fallback.mono && "font-mono text-12")}>{name ?? fallback.text}</span>
        <span className="block text-12 wrap-anywhere text-muted">
          {name !== null && <span className="mr-1.5 font-mono">{fallback.text}</span>}
          {detail}
        </span>
      </span>
    </>
  );
}

function LinkRowBody({ item }: { item: Extract<OmniboxItem, { type: "link" }> }) {
  const Icon = item.kind === "page" ? ArrowRight : item.kind === "search" ? Search : ENTITY_ICON[item.kind];
  const color = item.kind === "page" || item.kind === "search" ? "text-muted" : ENTITY_TEXT[item.kind];
  const mono = item.kind !== "page" && item.kind !== "search" && item.kind !== "name";
  return (
    <>
      <span className={cx("flex size-7 items-center justify-center rounded-md bg-surface-2", color)}>
        <Icon className="size-3.5" />
      </span>
      <span className="min-w-0">
        <span className={cx("block font-medium wrap-anywhere", mono && "font-mono text-12")}>{item.label}</span>
        <span className="block text-12 wrap-anywhere text-muted">{item.detail}</span>
      </span>
    </>
  );
}

function StatusLine({ status }: { status: OmniboxStatus }) {
  const text = statusText(status);
  if (text === null) return null;
  if (status.kind === "unavailable" || status.kind === "failed") return <WarningLine>{text}</WarningLine>;
  return <div className="border-b border-border px-3.5 py-1.5 text-11 text-subtle">{text}</div>;
}

function FailureLine({ summary }: { summary: string | null }) {
  if (summary === null) return null;
  return <WarningLine>{summary}</WarningLine>;
}

function WarningLine({ children }: { children: ReactNode }) {
  return (
    <div className="border-b border-border px-3.5 py-1.5 text-11 text-warning">
      {children}
    </div>
  );
}

function statusText(status: OmniboxStatus): string | null {
  switch (status.kind) {
    case "idle":
      return null;
    case "searching":
      return "Searching…";
    case "ready":
      return status.count === 0 ? "No matches on the server." : null;
    case "unavailable":
      return `Server search is not available yet (${status.error.status} ${status.error.code}). Suggestions below come from the shape of the input.`;
    case "failed":
      return `Search failed: ${status.error.message}. Suggestions below come from the shape of the input.`;
  }
}

function linkType(item: Extract<OmniboxItem, { type: "link" }>): string {
  if (item.kind === "page") return "Page";
  if (item.kind === "search") return "Action";
  return item.kind === "name" ? "Requests" : ENTITY_KIND_LABEL[item.kind];
}
