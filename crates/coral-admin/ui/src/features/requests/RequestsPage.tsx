import { RefreshCw } from "lucide-react";

import type { RequesterGroup } from "../../api/types/request-explorer";
import { useSession } from "../../app/boot";
import { Button } from "../../kit/Button";
import { Segmented } from "../../kit/Segmented";
import type { CoverageFeature } from "../../lib/coverage";
import { useTimeFormat } from "../../lib/time";
import { groupGaps, groupQueryOf, groupSelection, histogramFilterOf, histogramGaps, requestQueryOf, type ExplorerFilter, type ExplorerView } from "./explorer-filter";
import { FilterBar } from "./FilterBar";
import { GroupsView } from "./GroupsView";
import { HistogramStrip } from "./HistogramStrip";
import { RequestList } from "./RequestList";
import { useExplorerState } from "./useExplorerState";

const VIEW_OPTIONS: { value: ExplorerView; label: string }[] = [
  { value: "list", label: "List" },
  { value: "groups", label: "Requester groups" },
];

export function RequestsPage() {
  const state = useExplorerState();
  const { coverage } = useSession();
  const { time } = useTimeFormat();
  const { filter, q, bounds, anchorMs } = state;
  const topMs = bounds.to === null ? anchorMs : Date.parse(bounds.to);
  const live = state.live && filter.range.kind === "window";
  const openGroup = (group: RequesterGroup) => state.setFilter({ ...filter, ...groupSelection(group) }, "list");

  return (
    <>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h1 className="m-0 text-22 font-semibold">Requests</h1>
          <p className="m-0 text-muted">
            Every request to the API, newest first. Click a row for a preview; open it for every header and the full body.
            {filter.range.kind === "window" && !live && <span className="text-subtle"> Window ends {time(new Date(anchorMs).toISOString(), { seconds: true })}.</span>}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {filter.range.kind === "window" && !live && (
            <Button size="sm" icon={<RefreshCw className="size-3" />} onClick={state.refresh}>
              Refresh
            </Button>
          )}
          <Segmented label="View" value={state.view} options={VIEW_OPTIONS} onChange={state.setView} />
        </div>
      </div>
      <FilterBar filter={filter} q={q} live={state.live} onFilterChange={state.setFilter} onQChange={state.setQ} onLiveChange={state.setLive} />
      <section aria-label="Requests over time" className="rounded-md border border-border bg-surface px-3.5 py-3">
        <HistogramStrip
          filter={histogramFilterOf(filter, bounds, anchorMs)}
          unsupported={histogramGaps(filter, q)}
          onZoom={(range) => state.setFilter({ ...filter, range: { kind: "custom", ...range } })}
        />
      </section>
      {state.view === "list" ? (
        <RequestList
          query={requestQueryOf(filter, q, bounds)}
          live={live}
          topMs={topMs}
          coverage={coverage}
          features={listFeatures(filter)}
          selected={state.selected}
          onSelect={state.setSelected}
          onRebase={state.refresh}
        />
      ) : (
        <GroupsView query={groupQueryOf(filter, bounds, anchorMs, state.sort)} coverage={coverage} unsupported={groupGaps(filter, q)} onSortChange={state.setSort} onPick={openGroup} />
      )}
    </>
  );
}

function listFeatures(filter: ExplorerFilter): CoverageFeature[] {
  const attributed = filter.route !== null || filter.cause !== null || filter.keyKind !== null || filter.country !== null || filter.requester?.kind === "actor" || filter.requester?.kind === "player";
  const features: CoverageFeature[] = ["log"];
  if (attributed) features.push("attribution");
  if (filter.player !== null) features.push("subjects", "body_subjects");
  return features;
}
