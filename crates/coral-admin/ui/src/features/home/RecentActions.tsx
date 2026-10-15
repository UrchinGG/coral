import { useQuery } from "@tanstack/react-query";

import { recentActionsQuery } from "../../api/actions";
import type { AdminAction } from "../../api/types/actions";
import { EntityLink } from "../../kit/EntityLink";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { SkeletonRows } from "../../kit/States";
import { useTimeFormat } from "../../lib/time";

const WORD_SEPARATOR = /_/g;

export function RecentActions() {
  const actions = useQuery(recentActionsQuery());
  return (
    <Section title="Recent admin actions" count={actions.data?.items.length} flush fetching={actions.isFetching && actions.data !== undefined}>
      <QueryContent query={actions} skeleton={<SkeletonRows count={6} />} isEmpty={(data) => data.items.length === 0} empty="No admin actions recorded yet.">
        {(data) => (
          <ul className="m-0 flex list-none flex-col p-0">
            {data.items.map((action) => (
              <ActionRow key={action.id} action={action} />
            ))}
          </ul>
        )}
      </QueryContent>
    </Section>
  );
}

function ActionRow({ action }: { action: AdminAction }) {
  const { time } = useTimeFormat();
  return (
    <li className="grid grid-cols-[52px_minmax(0,1fr)] gap-2.5 border-b border-border px-3.5 py-[7px] text-12 last:border-b-0">
      <time dateTime={action.at} className="font-mono text-11 text-subtle">
        {time(action.at, { seconds: false })}
      </time>
      <span className="flex min-w-0 flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
        <EntityLink entity={action.actor} />
        <span>{action.action.replace(WORD_SEPARATOR, " ")}</span>
        {action.target === null ? <span className="font-mono text-11 wrap-anywhere text-muted">{action.target_text}</span> : <EntityLink entity={action.target} />}
      </span>
    </li>
  );
}
