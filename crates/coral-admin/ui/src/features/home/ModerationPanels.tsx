import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { recentModerationQuery } from "../../api/moderation";
import type { PluginChange, PluginChangeKind, RecentModeration } from "../../api/types/moderation";
import type { EntityRef } from "../../api/types/refs";
import { EntityLink } from "../../kit/EntityLink";
import { KeyValue } from "../../kit/KeyValue";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { SkeletonLines } from "../../kit/States";
import { formatNumber } from "../../lib/format";
import { refIdentity } from "../../lib/refs";
import { useTimeFormat } from "../../lib/time";

const CHANGE_VERB: Record<PluginChangeKind, string> = {
  published: "published",
  yanked: "yanked",
  disabled: "disabled",
  unlisted: "unlisted",
};

export function ModerationPanels() {
  const moderation = useQuery(recentModerationQuery());
  return (
    <>
      <Section
        title="Moderation, 24 h"
        flush
        fetching={refetching(moderation)}
        actions={
          <Link to="/moderation" className="text-12 text-accent hover:underline">
            Open
          </Link>
        }
      >
        <div className="px-3.5 py-3">
          <QueryContent query={moderation} skeleton={<SkeletonLines />}>
            {(data) => <ModerationSummary moderation={data} />}
          </QueryContent>
        </div>
      </Section>
      <Section title="Plugin changes" count="7 days" flush fetching={refetching(moderation)}>
        <QueryContent
          query={moderation}
          skeleton={<SkeletonLines />}
          isEmpty={(data) => data.plugins.changes.length === 0}
          empty="No plugin was published, yanked, disabled or unlisted in the last 7 days."
        >
          {(data) => (
            <ul className="m-0 flex list-none flex-col p-0">
              {data.plugins.changes.map((change) => (
                <PluginChangeRow key={`${change.at}:${refIdentity(change.plugin)}:${change.change}`} change={change} />
              ))}
            </ul>
          )}
        </QueryContent>
      </Section>
    </>
  );
}

function ModerationSummary({ moderation }: { moderation: RecentModeration }) {
  const { relative } = useTimeFormat();
  const { tags, locks, strikes, reviews } = moderation;
  const byType = tags.by_type.map((count) => `${formatNumber(count.count)} ${count.tag_type ?? "untyped"}`).join(", ");
  return (
    <KeyValue
      items={[
        { label: "Tags added", value: <Counted count={tags.added} detail={byType} /> },
        { label: "Tags removed", value: <Counted count={tags.removed} /> },
        { label: "Strikes", value: <Counted count={strikes.issued} refs={strikes.accounts} /> },
        { label: "Players locked", value: <Counted count={locks.locked} refs={locks.players} detail={locks.unlocked > 0 ? `${formatNumber(locks.unlocked)} unlocked` : undefined} /> },
        {
          label: "Open reviews",
          value: <Counted count={reviews.open} detail={reviews.oldest_open === null ? undefined : `oldest opened ${relative(reviews.oldest_open)}`} />,
        },
      ]}
    />
  );
}

function Counted({ count, detail, refs = [] }: { count: number; detail?: string; refs?: readonly EntityRef[] }) {
  return (
    <span className="inline-flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
      <b>{formatNumber(count)}</b>
      {detail !== undefined && detail !== "" && <span className="text-muted">· {detail}</span>}
      {refs.map((ref) => (
        <EntityLink key={refIdentity(ref)} entity={ref} />
      ))}
    </span>
  );
}

function PluginChangeRow({ change }: { change: PluginChange }) {
  const { time } = useTimeFormat();
  return (
    <li className="grid grid-cols-[86px_minmax(0,1fr)] gap-2.5 border-b border-border px-3.5 py-[7px] text-12 last:border-b-0">
      <time dateTime={change.at} className="font-mono text-11 text-subtle">
        {time(change.at, { seconds: false, date: "always" })}
      </time>
      <span className="flex min-w-0 flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
        <EntityLink entity={change.plugin} />
        {change.version !== null && <span className="font-mono">{change.version}</span>}
        <span>{CHANGE_VERB[change.change]}</span>
        {change.by !== null && (
          <>
            <span className="text-subtle">by</span>
            <EntityLink entity={change.by} />
          </>
        )}
        {change.reason !== null && <span className="text-muted">· {change.reason}</span>}
      </span>
    </li>
  );
}

function refetching(query: UseQueryResult<unknown, Error>): boolean {
  return query.isFetching && query.data !== undefined;
}
