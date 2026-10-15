import { useQuery } from "@tanstack/react-query";
import { lazy, Suspense } from "react";
import { useParams } from "react-router-dom";

import { playerCoreQuery } from "../../api/players";
import type { PlayerCore } from "../../api/types/players";
import { EmptyState, SkeletonLines } from "../../kit/States";
import { Tabs, type TabItem } from "../../kit/Tabs";
import { normalizeUuid } from "../../lib/uuid";
import { useRecordRecent } from "../../omnibox/recent";
import { PlayerHeader } from "./PlayerHeader";
import { PlayerRail } from "./PlayerRail";
import { usePlayerTab, type PlayerTab } from "./player-state";
import { RequestsTab } from "./RequestsTab";

const TagsTab = lazy(() => import("./TagsTab").then((module) => ({ default: module.TagsTab })));
const SnapshotsTab = lazy(() => import("./SnapshotsTab").then((module) => ({ default: module.SnapshotsTab })));

export function PlayerPage() {
  const { uuid: raw = "" } = useParams();
  const uuid = normalizeUuid(raw);
  if (uuid === null) return <EmptyState>"{raw}" is not a player UUID. Search for a name with the omnibox instead.</EmptyState>;
  return <Player key={uuid} uuid={uuid} />;
}

function Player({ uuid }: { uuid: string }) {
  const core = useQuery(playerCoreQuery(uuid));
  const [tab, setTab] = usePlayerTab();
  useRecordRecent({ kind: "player", uuid });

  return (
    <>
      <PlayerHeader uuid={uuid} core={core} />
      <div className="grid min-w-0 gap-4 lg:grid-cols-[300px_minmax(0,1fr)] xl:grid-cols-[340px_minmax(0,1fr)]">
        <PlayerRail core={core} />
        <div className="flex min-w-0 flex-col gap-4">
          <Tabs label="Player sections" value={tab} tabs={tabsOf(core.data)} onChange={setTab} idPrefix="player" />
          <div id="player-panel" role="tabpanel" aria-labelledby={`player-tab-${tab}`} className="min-w-0">
            <Suspense fallback={<SkeletonLines />}>
              <TabPanel tab={tab} uuid={uuid} core={core.data} />
            </Suspense>
          </div>
        </div>
      </div>
    </>
  );
}

function TabPanel({ tab, uuid, core }: { tab: PlayerTab; uuid: string; core: PlayerCore | undefined }) {
  switch (tab) {
    case "requests":
      return <RequestsTab uuid={uuid} core={core} />;
    case "tags":
      return <TagsTab uuid={uuid} />;
    case "snapshots":
      return <SnapshotsTab uuid={uuid} />;
  }
}

function tabsOf(core: PlayerCore | undefined): readonly TabItem<PlayerTab>[] {
  return [
    { value: "requests", label: "Requests", count: core?.requests.last_14d },
    { value: "tags", label: "Tags", count: core?.blacklist.active.length },
    { value: "snapshots", label: "Snapshots", count: core?.snapshots.count },
  ];
}
