import { keepPreviousData, useQuery } from "@tanstack/react-query";

import { recentlyTaggedQuery, topPlayersQuery } from "../../api/players";
import type { PlayerRef } from "../../api/types/refs";
import type { TopWindow } from "../../api/types/players";
import { EntityLink } from "../../kit/EntityLink";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { Segmented } from "../../kit/Segmented";
import { EmptyState } from "../../kit/States";
import { formatNumber } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { useUrlEnum } from "../../lib/url-state";
import { useRecent } from "../../omnibox/recent";
import { DEFAULT_TOP_WINDOW, TOP_WINDOWS } from "./player-state";
import { TagPill } from "./TagPill";

const TOP_OPTIONS = TOP_WINDOWS.map((value) => ({ value, label: value }));
const LIST_ROW = "flex min-h-row items-center justify-between gap-2 border-b border-border px-3.5 py-1 text-12 last:border-b-0";

export function PlayersPage() {
  return (
    <>
      <div className="flex flex-col gap-1">
        <h1 className="text-22 font-semibold">Players</h1>
        <p className="m-0 text-muted">Minecraft players known from snapshots, tags or requests. Search for any player with the omnibox.</p>
      </div>
      <div className="grid min-w-0 gap-4 lg:grid-cols-3">
        <RecentlyViewed />
        <TopLookedUp />
        <RecentlyTagged />
      </div>
    </>
  );
}

function RecentlyViewed() {
  const players = useRecent().filter((ref): ref is PlayerRef => ref.kind === "player");
  return (
    <Section title="Recently viewed" count={players.length} flush>
      {players.length === 0 ? (
        <EmptyState>Players you open appear here.</EmptyState>
      ) : (
        <ul className="m-0 list-none p-0">
          {players.map((player) => (
            <li key={player.uuid} className={LIST_ROW}>
              <EntityLink entity={player} />
            </li>
          ))}
        </ul>
      )}
    </Section>
  );
}

function TopLookedUp() {
  const [topWindow, setTopWindow] = useUrlEnum<TopWindow>("top", TOP_WINDOWS, DEFAULT_TOP_WINDOW);
  const top = useQuery({ ...topPlayersQuery(topWindow), placeholderData: keepPreviousData });
  return (
    <Section
      title="Top looked up"
      count="excludes batch bodies"
      flush
      fetching={top.isFetching && top.data !== undefined}
      actions={<Segmented label="Top window" value={topWindow} options={TOP_OPTIONS} onChange={setTopWindow} />}
    >
      <QueryContent query={top} isEmpty={(data) => data.players.length === 0} empty="No player was looked up in this window.">
        {(data) => (
          <ul className="m-0 list-none p-0">
            {data.players.map((ranked) => (
              <li key={ranked.player} className={LIST_ROW}>
                <EntityLink entity={{ kind: "player", uuid: ranked.player }} />
                <span className="font-mono tabular-nums">{formatNumber(ranked.requests)}</span>
              </li>
            ))}
          </ul>
        )}
      </QueryContent>
    </Section>
  );
}

function RecentlyTagged() {
  const { time } = useTimeFormat();
  const tagged = useQuery(recentlyTaggedQuery());
  return (
    <Section title="Recently tagged" flush fetching={tagged.isFetching && tagged.data !== undefined}>
      <QueryContent query={tagged} isEmpty={(data) => data.items.length === 0} empty="No player carries a tag.">
        {(data) => (
          <ul className="m-0 list-none p-0">
            {data.items.map((entry) => (
              <li key={entry.id} className={LIST_ROW}>
                <span className="inline-flex min-w-0 flex-wrap items-center gap-1.5 text-muted">
                  <EntityLink entity={entry.player} />
                  <TagPill tag={entry} />
                  {entry.author !== null && (
                    <>
                      by <EntityLink entity={entry.author} />
                    </>
                  )}
                </span>
                <span className="font-mono text-11 whitespace-nowrap text-subtle">{time(entry.at, { seconds: false, date: "always" })}</span>
              </li>
            ))}
          </ul>
        )}
      </QueryContent>
    </Section>
  );
}
