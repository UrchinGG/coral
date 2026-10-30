import type { UseQueryResult } from "@tanstack/react-query";
import { Ban, Box, Lock } from "lucide-react";
import type { ReactNode } from "react";

import type { LockView, PlayerCore, PlayerName } from "../../api/types/players";
import { CopyField } from "../../kit/Copy";
import { EntityLink } from "../../kit/EntityLink";
import { Pill } from "../../kit/Badges";
import { ErrorState, Skeleton } from "../../kit/States";
import { formatNumber } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { dashUuid } from "../../lib/uuid";
import { PlayerActions } from "./PlayerActions";
import { hasProfile } from "./profile";
import { TagPill } from "./TagPill";

type PlayerHeaderProps = { uuid: string; core: UseQueryResult<PlayerCore, Error> };

export function PlayerHeader({ uuid, core }: PlayerHeaderProps) {
  const data = core.data;
  return (
    <header className="flex flex-col gap-3 rounded-md border border-border bg-surface px-4 py-3.5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <div aria-hidden className="mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-md bg-entity-player/12 text-entity-player">
            <Box className="size-4" />
          </div>
          <div className="flex min-w-0 flex-col gap-2">
            <Title uuid={uuid} core={data} />
            <Identifiers uuid={uuid} />
          </div>
        </div>
        {data !== undefined && <PlayerActions uuid={uuid} locked={data.blacklist.locked} blocked={data.minecraft_block} />}
      </div>
      {data === undefined && core.error === null && <Skeleton className="h-3 w-2/3" />}
      {data === undefined && core.error !== null && <ErrorState error={core.error} onRetry={() => void core.refetch()} retrying={core.isFetching} />}
      {data !== undefined && <Facts core={data} />}
    </header>
  );
}

function Title({ uuid, core }: { uuid: string; core: PlayerCore | undefined }) {
  const name = core?.name ?? null;
  return (
    <div className="flex flex-wrap items-center gap-2">
      {name === null ? <h1 className="font-mono text-18 font-semibold break-all">{uuid}</h1> : <h1 className="text-22 font-semibold">{name}</h1>}
      {core?.blacklist.active.map((tag) => (
        <TagPill key={tag.id} tag={tag} />
      ))}
      {core !== undefined && core.blacklist.locked !== null && (
        <HeldBy hold={core.blacklist.locked} icon={<Lock className="size-3" />} label="locked" title="Tags on this player can only be changed by staff" />
      )}
      {core !== undefined && core.minecraft_block !== null && (
        <HeldBy hold={core.minecraft_block} icon={<Ban className="size-3" />} label="sign-in blocked" title={core.minecraft_block.reason ?? "This Minecraft account cannot sign in to Urchin"} />
      )}
    </div>
  );
}

function HeldBy({ hold, icon, label, title }: { hold: LockView; icon: ReactNode; label: string; title: string }) {
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5 text-12 text-muted">
      <Pill tone="neutral" title={title}>
        {icon}
        {label}
      </Pill>
      {hold.by !== null && (
        <>
          by <EntityLink entity={hold.by} />
        </>
      )}
    </span>
  );
}

function Identifiers({ uuid }: { uuid: string }) {
  return (
    <div className="flex flex-wrap gap-x-5 gap-y-1 text-12">
      <span className="flex items-center gap-2">
        <span className="text-subtle">Undashed</span>
        <CopyField value={uuid} label="Copy undashed UUID" />
      </span>
      <span className="flex items-center gap-2">
        <span className="text-subtle">Dashed</span>
        <CopyField value={dashUuid(uuid)} label="Copy dashed UUID" />
      </span>
    </div>
  );
}

function Facts({ core }: { core: PlayerCore }) {
  const { time } = useTimeFormat();
  const stamp = (iso: string) => time(iso, { seconds: false, date: "always" });
  if (!hasProfile(core)) return <p className="m-0 text-12 text-muted">No profile — known only from requests</p>;
  const former = core.names.filter((name) => !name.current);
  const current = core.names.find((name) => name.current);
  return (
    <div className="flex flex-wrap gap-x-5 gap-y-1.5 text-12 text-muted">
      <span>
        {former.length === 0 ? (
          "No former names"
        ) : (
          <>
            Former names:{" "}
            {former.map((name, index) => (
              <span key={name.name}>
                {index > 0 && " · "}
                <EntityLink entity={{ kind: "name", name: name.name.toLowerCase() }} label={name.name} mono />{" "}
                <span className="text-subtle">{heldWindow(name, stamp)}</span>
              </span>
            ))}
          </>
        )}
      </span>
      {current !== undefined && current.first_seen !== null && <span>Current name since {stamp(current.first_seen)}</span>}
      {core.snapshots.first !== null && <span>First seen {stamp(core.snapshots.first)}</span>}
      <span>{formatNumber(core.snapshots.count)} snapshots</span>
    </div>
  );
}

function heldWindow(name: PlayerName, stamp: (iso: string) => string): string {
  const from = name.first_seen === null ? "before the first snapshot" : stamp(name.first_seen);
  const until = name.last_seen === null ? "now" : stamp(name.last_seen);
  return `${from} → ${until}`;
}
