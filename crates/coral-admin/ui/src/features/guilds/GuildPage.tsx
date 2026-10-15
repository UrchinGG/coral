import { useQuery } from "@tanstack/react-query";
import { useParams } from "react-router-dom";

import { guildQuery, guildSnapshotQuery } from "../../api/guilds";
import type { GuildDetail } from "../../api/types/platform";
import { KpiStrip, KpiTile } from "../../kit/KpiTile";
import { LazyJsonView } from "../../kit/LazyJsonView";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { Select } from "../../kit/Select";
import { formatNumber } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { useUrlParam } from "../../lib/url-state";
import { PageHead } from "../platform/PageHead";
import { When } from "../platform/When";

const LATEST = "";
const JSON_LINKS = { players: true } as const;

export function GuildPage() {
  const { guildId = "" } = useParams();
  const guild = useQuery(guildQuery(guildId));
  return <QueryContent query={guild}>{(data) => <GuildView detail={data} />}</QueryContent>;
}

function GuildView({ detail }: { detail: GuildDetail }) {
  const { summary } = detail;
  const guildId = detail.guild.guild_id;
  return (
    <>
      <PageHead title={detail.guild.name ?? guildId} lede={<span className="font-mono">{guildId}</span>} />
      {summary !== null && (
        <KpiStrip compact>
          <KpiTile label="Tag" value={summary.tag === null ? "—" : <span className="font-mono">[{summary.tag}]</span>} />
          <KpiTile label="Level" value={formatNumber(summary.level)} />
          <KpiTile label="Members" value={formatNumber(summary.member_count)} />
          <KpiTile label="Experience" value={formatNumber(summary.experience)} />
          <KpiTile label="Refreshed" value={<When at={summary.updated_at} />} />
        </KpiStrip>
      )}
      <Snapshots guildId={guildId} detail={detail} />
    </>
  );
}

function Snapshots({ guildId, detail }: { guildId: string; detail: GuildDetail }) {
  const [at, setAt] = useUrlParam("at");
  const { time } = useTimeFormat();
  const historical = useQuery({ ...guildSnapshotQuery(guildId, at ?? LATEST), enabled: at !== null });
  const options = [{ value: LATEST, label: "Latest" }, ...detail.snapshots.map((ts) => ({ value: ts, label: time(ts, { date: "always", zone: true }) }))];
  return (
    <Section
      title="Snapshot"
      count={`${formatNumber(detail.snapshots.length)} recorded`}
      actions={<Select aria-label="Snapshot time" value={at ?? LATEST} options={options} onChange={(value) => setAt(value === LATEST ? null : value)} />}
      fetching={historical.isFetching && historical.data !== undefined}
    >
      {at === null ? (
        <LazyJsonView value={detail.current} label="Latest guild snapshot" links={JSON_LINKS} />
      ) : (
        <QueryContent query={historical}>{(snapshot) => <LazyJsonView value={snapshot.snapshot} label="Guild snapshot" links={JSON_LINKS} />}</QueryContent>
      )}
    </Section>
  );
}
