import { useQuery } from "@tanstack/react-query";
import { useMemo, type ReactNode } from "react";

import { playerTagsQuery, usePlayerMutations } from "../../api/players";
import type { LockEvent, TagEnding, TagRecord, TagView } from "../../api/types/players";
import { Pill } from "../../kit/Badges";
import { ClampText } from "../../kit/ClampText";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { EntityLink } from "../../kit/EntityLink";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { Table, type Column } from "../../kit/Table";
import { Timeline, type TimelineItem } from "../../kit/Timeline";
import { toast } from "../../kit/toast";
import { useTimeFormat } from "../../lib/time";
import { TagPill } from "./TagPill";

const ENDING_LABEL: Record<TagEnding["how"], string> = { cleared: "removed", replaced: "replaced", expired: "expired" };

export function TagsTab({ uuid }: { uuid: string }) {
  const tags = useQuery(playerTagsQuery(uuid));
  return (
    <QueryContent query={tags}>
      {(data) => (
        <div className="flex flex-col gap-4">
          <ActiveTags uuid={uuid} active={data.active} />
          <TagHistory history={data.history} />
          <LockHistory locks={data.locks} />
        </div>
      )}
    </QueryContent>
  );
}

function ActiveTags({ uuid, active }: { uuid: string; active: readonly TagView[] }) {
  const { time } = useTimeFormat();
  const { removeTag } = usePlayerMutations(uuid);
  const remove = (tag: TagView) =>
    removeTag.mutate(tag.tag_type, {
      onSuccess: () => toast.ok(`Removed the ${tag.display_name} tag.`),
      onError: (error) => toast.error(`Could not remove the ${tag.display_name} tag: ${error.message}`),
    });
  return (
    <Section title="Active tags" count={active.length}>
      {active.length === 0 ? (
        <span className="text-12 text-subtle">No active tags</span>
      ) : (
        <div className="flex flex-col gap-3 text-12">
          {active.map((tag) => (
            <div key={tag.id} className="flex flex-wrap items-start justify-between gap-3">
              <div className="flex min-w-0 flex-col gap-1">
                <span className="flex flex-wrap items-center gap-1.5 text-muted">
                  <TagPill tag={tag} />
                  {tag.author !== null && (
                    <>
                      by <EntityLink entity={tag.author} />
                    </>
                  )}
                  <span className="text-subtle">{time(tag.at, { seconds: false, date: "always" })}</span>
                  {tag.hide_username && <Pill tone="neutral">username hidden</Pill>}
                  {tag.expires_at !== null && <span className="text-subtle">expires {time(tag.expires_at, { seconds: false, date: "always" })}</span>}
                </span>
                {tag.reason !== null && <p className="m-0 wrap-anywhere">{tag.reason}</p>}
              </div>
              <ConfirmButton size="sm" variant="danger" confirmLabel="Remove" pending={removeTag.isPending && removeTag.variables === tag.tag_type} onConfirm={() => remove(tag)}>
                Remove
              </ConfirmButton>
            </div>
          ))}
        </div>
      )}
    </Section>
  );
}

function TagHistory({ history }: { history: readonly TagRecord[] }) {
  const columns = useHistoryColumns();
  return (
    <Section title="Tag history" count={history.length} flush>
      <Table label="Tag history" columns={columns} rows={history} rowKey={(record) => String(record.id)} empty="No tags, current or removed." />
    </Section>
  );
}

function useHistoryColumns(): readonly Column<TagRecord>[] {
  const { time } = useTimeFormat();
  return useMemo(() => {
    const stamp = (iso: string) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(iso, { seconds: false, date: "always" })}</span>;
    return [
      { id: "tag", header: "Tag", cell: (record) => <TagPill tag={record} /> },
      { id: "reason", header: "Reason", cell: (record) => (record.reason === null ? <span className="text-subtle">none</span> : <ClampText>{record.reason}</ClampText>) },
      { id: "by", header: "By", cell: (record) => (record.author === null ? <span className="text-subtle">unknown</span> : <EntityLink entity={record.author} />), priority: 2 },
      { id: "added", header: "Added", cell: (record) => stamp(record.at), priority: 2 },
      { id: "ended", header: "Ended", cell: (record) => <Ending ending={record.ended} stamp={stamp} /> },
    ];
  }, [time]);
}

function Ending({ ending, stamp }: { ending: TagEnding | null; stamp: (iso: string) => ReactNode }) {
  if (ending === null) return <Pill tone="ok">active</Pill>;
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5 text-muted">
      {ENDING_LABEL[ending.how]} {stamp(ending.at)}
      {ending.by !== null && (
        <>
          by <EntityLink entity={ending.by} />
        </>
      )}
    </span>
  );
}

function LockHistory({ locks }: { locks: readonly LockEvent[] }) {
  const items: TimelineItem[] = locks.map((lock, index) => ({
    id: `${lock.at}:${index}`,
    at: lock.at,
    tone: lock.locked ? "warning" : "ok",
    title: (
      <span>
        {lock.locked ? "Locked" : "Unlocked"}
        {lock.by !== null && (
          <>
            {" by "}
            <EntityLink entity={lock.by} />
          </>
        )}
      </span>
    ),
    detail: lock.reason ?? undefined,
  }));
  return (
    <Section title="Lock events" count={locks.length} flush>
      {items.length === 0 ? <div className="px-3.5 py-3 text-12 text-subtle">Never locked</div> : <Timeline items={items} />}
    </Section>
  );
}
