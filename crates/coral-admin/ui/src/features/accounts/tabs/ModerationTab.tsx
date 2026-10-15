import { useInfiniteQuery, type InfiniteData, type UseInfiniteQueryResult } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { KEY_PREFIX_LENGTH } from "../../../api/apiKeys";
import { timelineQuery } from "../../../api/accounts";
import type { AccountCore, CursorPage, TimelineEvent } from "../../../api/types/accounts";
import type { EntityRef } from "../../../api/types/refs";
import { Badge } from "../../../kit/Badges";
import { Button } from "../../../kit/Button";
import { EntityLink } from "../../../kit/EntityLink";
import { Section } from "../../../kit/Section";
import { EmptyState, ErrorState, SkeletonLines } from "../../../kit/States";
import { Timeline, type TimelineItem } from "../../../kit/Timeline";
import type { Tone } from "../../../kit/tone";
import { isSnowflake } from "../../../lib/refs";
import { normalizeUuid } from "../../../lib/uuid";

type TimelineQuery = UseInfiniteQueryResult<InfiniteData<CursorPage<TimelineEvent>>, Error>;

const RAISING_EVENTS = new Set(["tag_set", "lock"]);
const PLAYER_DETAILS = new Set(["uuid", "player"]);
const ACCOUNT_DETAILS = new Set(["source"]);

export function ModerationTab({ core }: { core: AccountCore }) {
  const timeline = useInfiniteQuery(timelineQuery(core.account.discord_id));
  const events = timeline.data?.pages.flatMap((page) => page.items) ?? [];
  return (
    <Section
      title="Moderation timeline"
      count={events.length === 0 ? undefined : events.length}
      flush
      fetching={timeline.isFetching && timeline.data !== undefined}
      actions={<span className="text-11 text-subtle">admin and bot actions, strikes, tags, reviews, votes, key and link changes</span>}
    >
      <TimelineBody timeline={timeline} events={events} />
    </Section>
  );
}

function TimelineBody({ timeline, events }: { timeline: TimelineQuery; events: readonly TimelineEvent[] }) {
  if (events.length === 0) {
    if (timeline.error !== null) return <ErrorState error={timeline.error} onRetry={() => void timeline.refetch()} />;
    if (timeline.isPending) return <SkeletonLines />;
    return <EmptyState>Nothing on record for this account.</EmptyState>;
  }
  return (
    <>
      <Timeline items={events.map(timelineItem)} />
      {timeline.error !== null && <ErrorState error={timeline.error} onRetry={() => void timeline.fetchNextPage()} title="Loading older events failed" />}
      {timeline.hasNextPage && (
        <div className="border-t border-border px-3.5 py-2">
          <Button size="sm" onClick={() => void timeline.fetchNextPage()} pending={timeline.isFetchingNextPage}>
            Load older events
          </Button>
        </div>
      )}
    </>
  );
}

function timelineItem(event: TimelineEvent): TimelineItem {
  const { title, tone } = describe(event);
  return { id: event.key, at: event.at, tone, title, detail: event.detail ?? undefined };
}

function describe(event: TimelineEvent): { title: ReactNode; tone: Tone } {
  const actor = event.actor === null ? null : <EntityLink entity={event.actor} />;
  const player = event.player === null ? null : <EntityLink entity={event.player} />;
  const target = targetOf(event);
  const action = <Badge>{event.kind === "player_event" || event.kind.startsWith("tag_") ? eventLabel(event) : event.action}</Badge>;
  switch (event.kind) {
    case "admin_action":
      return { title: <Line>{actor}{action}{target}{dataSummary(event.data)}</Line>, tone: "info" };
    case "strike_received":
      return { title: <Line>Strike issued by{actor}</Line>, tone: "danger" };
    case "strike_revoked":
      return { title: <Line>Strike revoked by{actor}</Line>, tone: "ok" };
    case "strike_issued":
      return { title: <Line>Struck{target}</Line>, tone: "warning" };
    case "player_event":
      return { title: <Line>{player}{action}by{actor}</Line>, tone: playerEventTone(event.action) };
    case "tag_authored":
      return { title: <Line>Authored{action}on{player}</Line>, tone: "neutral" };
    case "tag_reviewed":
      return { title: <Line>Reviewed{action}on{player}by{actor}</Line>, tone: "neutral" };
    case "review_submitted":
      return { title: <Line>Submitted a review of{player}{action}</Line>, tone: "neutral" };
    case "vote":
      return { title: <Line>Voted{action}on the review of{player}</Line>, tone: "neutral" };
    case "key_created":
      return { title: <Line>{`${event.action ?? "API"} key created`}{keyLink(event)}</Line>, tone: "ok" };
    case "key_retired":
      return { title: <Line>{`${event.action ?? "API"} key retired`}{keyLink(event)}</Line>, tone: "neutral" };
    case "linked":
      return { title: <Line>Linked{player}{`as ${event.action ?? "account"}`}</Line>, tone: "info" };
    case "unlinked":
      return { title: <Line>Unlinked{player}{`(${event.action ?? "account"})`}</Line>, tone: "warning" };
  }
}

function Line({ children }: { children: ReactNode }) {
  return <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">{children}</span>;
}

function targetOf(event: TimelineEvent): ReactNode {
  if (event.target !== null) return <EntityLink entity={event.target} />;
  if (event.target_text === null) return null;
  return <span className="font-mono">{event.target_text}</span>;
}

function eventLabel(event: TimelineEvent): string {
  const tagType = typeof event.data?.tag_type === "string" ? event.data.tag_type : null;
  return tagType === null ? (event.action ?? "event") : `${event.action ?? "event"} ${tagType}`;
}

function playerEventTone(action: string | null): Tone {
  return action !== null && RAISING_EVENTS.has(action) ? "danger" : "ok";
}

function keyLink(event: TimelineEvent): ReactNode {
  const keyId = event.data?.key_id;
  const prefix = event.data?.prefix;
  if (typeof keyId !== "number" || typeof prefix !== "string") return null;
  return <EntityLink entity={{ kind: "key", key_id: keyId, prefix: prefix.slice(0, KEY_PREFIX_LENGTH), key_kind: event.action === "developer" ? "developer" : "personal" }} />;
}

function dataSummary(data: TimelineEvent["data"]): ReactNode {
  if (data === null) return null;
  const pairs = Object.entries(data).filter(([, value]) => value !== null);
  if (pairs.length === 0) return null;
  return (
    <span className="inline-flex flex-wrap items-center gap-x-1.5 font-mono text-11 text-muted">
      {pairs.map(([name, value]) => (
        <DetailValue key={name} name={name} value={typeof value === "object" ? JSON.stringify(value) : String(value)} />
      ))}
    </span>
  );
}

function DetailValue({ name, value }: { name: string; value: string }) {
  const entity = detailEntity(name, value);
  return (
    <span className="inline-flex items-center gap-0.5 wrap-anywhere">
      {name}={entity === null ? value : <EntityLink entity={entity} />}
    </span>
  );
}

function detailEntity(name: string, value: string): EntityRef | null {
  const uuid = PLAYER_DETAILS.has(name) ? normalizeUuid(value) : null;
  if (uuid !== null) return { kind: "player", uuid };
  if (ACCOUNT_DETAILS.has(name) && isSnowflake(value)) return { kind: "account", discord_id: value, member_id: null };
  return null;
}
