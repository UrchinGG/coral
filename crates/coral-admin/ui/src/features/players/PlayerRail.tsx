import type { UseQueryResult } from "@tanstack/react-query";
import { TriangleAlert } from "lucide-react";

import type { PlayerCore, PlayerOwner } from "../../api/types/players";
import { Badge } from "../../kit/Badges";
import { EntityLink } from "../../kit/EntityLink";
import { KeyValue } from "../../kit/KeyValue";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { CoverageBanner } from "../../kit/States";
import { formatNumber, plural } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { TagPill } from "./TagPill";

export function PlayerRail({ core }: { core: UseQueryResult<PlayerCore, Error> }) {
  return (
    <aside className="flex min-w-0 flex-col gap-4" aria-label="Player summary">
      <QueryContent query={core}>
        {(data) => (
          <>
            <Owners owners={data.owners} />
            <Blacklist core={data} />
            <Snapshots core={data} />
          </>
        )}
      </QueryContent>
    </aside>
  );
}

function Owners({ owners }: { owners: readonly PlayerOwner[] }) {
  const current = owners.filter((owner) => owner.unlinked_at === null);
  const former = owners.filter((owner) => owner.unlinked_at !== null);
  const shared = current.find((owner) => owner.relation === "primary" && owner.primary_shared_with > 0);
  return (
    <Section title="Owners" count={current.length}>
      <div className="flex flex-col gap-2.5">
        {shared !== undefined && (
          <CoverageBanner tone="warning">
            This UUID is the primary account of {plural(shared.primary_shared_with + 1, "member")}.
          </CoverageBanner>
        )}
        {current.length === 0 ? <span className="text-12 text-subtle">No Coral account links this player</span> : current.map((owner) => <OwnerLine key={ownerKey(owner)} owner={owner} />)}
        {former.length > 0 && (
          <div className="flex flex-col gap-2 border-t border-border pt-2.5">
            <span className="text-11 font-semibold tracking-[0.06em] text-subtle uppercase">Former owners</span>
            {former.map((owner) => (
              <OwnerLine key={ownerKey(owner)} owner={owner} />
            ))}
          </div>
        )}
      </div>
    </Section>
  );
}

function OwnerLine({ owner }: { owner: PlayerOwner }) {
  const { time } = useTimeFormat();
  const stamp = (iso: string | null) => (iso === null ? "unknown" : time(iso, { seconds: false, date: "always" }));
  return (
    <div className="flex flex-col gap-0.5 text-12">
      <span className="flex flex-wrap items-center gap-1.5">
        <EntityLink entity={owner.account} />
        <Badge>{owner.relation}</Badge>
      </span>
      <span className="text-11 text-subtle">
        {owner.unlinked_at === null ? `linked ${stamp(owner.linked_at)}` : `${stamp(owner.linked_at)} → ${stamp(owner.unlinked_at)}`}
      </span>
    </div>
  );
}

function Blacklist({ core }: { core: PlayerCore }) {
  const { time } = useTimeFormat();
  const { active, locked } = core.blacklist;
  return (
    <Section title="Blacklist" count={active.length}>
      <div className="flex flex-col gap-3 text-12">
        {active.length === 0 && <span className="text-subtle">Not on the blacklist</span>}
        {active.map((tag) => (
          <div key={tag.id} className="flex flex-col gap-1">
            <span className="flex flex-wrap items-center gap-1.5 text-muted">
              <TagPill tag={tag} />
              {tag.author !== null && (
                <>
                  by <EntityLink entity={tag.author} />
                </>
              )}
              <span className="text-subtle">{time(tag.at, { seconds: false, date: "always" })}</span>
            </span>
            {tag.reason !== null && <p className="m-0 wrap-anywhere">{tag.reason}</p>}
          </div>
        ))}
        {locked !== null && (
          <span className="flex items-start gap-1.5 text-muted">
            <TriangleAlert className="mt-0.5 size-3 shrink-0 text-warning" />
            <span>
              {`Locked ${time(locked.at, { seconds: false, date: "always" })}`}
              {locked.by !== null && (
                <>
                  {" by "}
                  <EntityLink entity={locked.by} />
                </>
              )}
              {locked.reason !== null && <span className="text-fg">{` · ${locked.reason}`}</span>}
            </span>
          </span>
        )}
      </div>
    </Section>
  );
}

function Snapshots({ core }: { core: PlayerCore }) {
  const { time } = useTimeFormat();
  const { count, first, last } = core.snapshots;
  const day = (iso: string | null) => (iso === null ? "none" : time(iso, { seconds: false, date: "always" }));
  return (
    <Section title="Snapshots">
      <KeyValue
        items={[
          { label: "Range", value: count === 0 ? "No snapshots" : `${day(first)} → ${day(last)}` },
          { label: "Count", value: formatNumber(count) },
          { label: "Session markers", value: formatNumber(core.session_markers) },
          { label: "Lookups 24 h", value: formatNumber(core.requests.last_24h) },
          { label: "Lookups 14 d", value: `${formatNumber(core.requests.last_14d)} by ${plural(core.requests.requesters_14d, "requester")}` },
        ]}
      />
    </Section>
  );
}

function ownerKey(owner: PlayerOwner): string {
  return `${owner.account.discord_id}:${owner.relation}:${owner.linked_at ?? ""}`;
}
