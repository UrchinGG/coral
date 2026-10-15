import { Link } from "react-router-dom";

import type { EntityRef } from "../../api/types/refs";
import type { InvolvedPlayer, KeyDetail, RequestFull } from "../../api/types/request-full";
import type { RequestRow } from "../../api/types/requests";
import { Badge } from "../../kit/Badges";
import { CopyField } from "../../kit/Copy";
import { EntityLink } from "../../kit/EntityLink";
import { KeyValue, type KeyValueItem } from "../../kit/KeyValue";
import { Section } from "../../kit/Section";
import { formatNumber, plural } from "../../lib/format";
import { SERVICE_LABEL } from "../../lib/refs";
import { useTimeFormat } from "../../lib/time";
import { keyPrefixOf } from "./key-prefix";
import { KeyCell } from "./request-cells";
import { FORM_LABEL, subjectRef } from "./subjects";

const NEARBY_MS = 5 * 60_000;
const NONE = <span className="text-subtle">none</span>;

export function RequestRail({ full }: { full: RequestFull }) {
  return (
    <aside className="flex min-w-0 flex-col gap-4">
      <Section title="Attribution">
        <KeyValue items={attributionItems(full)} />
      </Section>
      <Players players={full.subjects} />
      <Context full={full} />
    </aside>
  );
}

function attributionItems(full: RequestFull): KeyValueItem[] {
  const { caller, request } = full;
  const service = request.requester.kind === "internal" ? SERVICE_LABEL[request.requester.service] : null;
  return [
    { key: "account", label: "Account", value: link(caller.account) },
    { key: "actor", label: "Actor", value: caller.actor === null ? NONE : <EntityLink entity={caller.actor} /> },
    ...(service === null ? [] : [{ key: "service", label: "Service", value: service }]),
    { key: "key", label: "Key", value: caller.key === null ? <KeyCell row={request} /> : <PresentedKey full={full} /> },
    { key: "kind", label: "Key kind", value: <span className="font-mono">{request.key_kind}</span> },
    { key: "ip", label: "IP", value: request.ip === null ? NONE : <IpLine full={full} /> },
    { key: "agent", label: "User agent", value: <span className="font-mono">{request.user_agent ?? "—"}</span> },
    { key: "starfish", label: "Starfish user", value: link(caller.starfish_user) },
    { key: "plugin", label: "Plugin", value: link(caller.plugin) },
  ];
}

function PresentedKey({ full }: { full: RequestFull }) {
  const key = full.caller.key;
  if (key === null) return NONE;
  const page = keyPageRef(key, full.request);
  return (
    <div className="flex flex-col gap-0.5">
      <CopyField value={key.key} label="Copy key" secret />
      <span className="inline-flex flex-wrap items-center gap-x-1.5 text-muted">
        {key.owner === null ? "matches no stored key" : <EntityLink entity={key.owner} />}
        <span>· {key.state}</span>
        {page !== null && (
          <>
            <span>·</span>
            <EntityLink entity={page} />
          </>
        )}
      </span>
    </div>
  );
}

function keyPageRef(key: KeyDetail, request: RequestRow): EntityRef | null {
  if (key.key_id !== null) return { kind: "key", key_id: key.key_id, prefix: keyPrefixOf(key.key), key_kind: request.key_kind };
  if (request.key_prefix === null) return null;
  return { kind: "key", key_id: null, prefix: request.key_prefix, key_kind: request.key_kind };
}

function IpLine({ full }: { full: RequestFull }) {
  const { ip, country } = full.request;
  if (ip === null) return NONE;
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5">
      <EntityLink entity={ip} />
      <span className="text-muted">· {country ?? "country unknown"}</span>
    </span>
  );
}

function Players({ players }: { players: readonly InvolvedPlayer[] }) {
  return (
    <Section title="Players involved" count={formatNumber(players.length)} flush>
      {players.length === 0 ? (
        <p className="m-0 px-3.5 py-3 text-12 text-subtle">No player subjects.</p>
      ) : (
        <ul className="m-0 flex max-h-[360px] list-none flex-col overflow-y-auto p-0">
          {players.map((player, index) => (
            <PlayerItem key={`${player.form}:${player.raw}:${index}`} player={player} />
          ))}
        </ul>
      )}
    </Section>
  );
}

function PlayerItem({ player }: { player: InvolvedPlayer }) {
  const ref = subjectRef(player);
  return (
    <li className="flex flex-wrap items-center gap-x-2 gap-y-0.5 border-b border-border px-3.5 py-1.5 text-12 last:border-b-0">
      {ref === null ? <span className="font-mono break-all">{player.raw}</span> : <EntityLink entity={ref} label={ref.kind === "name" ? player.raw : undefined} />}
      {renamed(player) && <span className="text-muted">now {player.current_name}</span>}
      <span className="grow" />
      <Badge>{FORM_LABEL[player.form]}</Badge>
      {player.player === null && <Badge variant="byname">by name</Badge>}
    </li>
  );
}

function Context({ full }: { full: RequestFull }) {
  const { time } = useTimeFormat();
  const { neighbors, request } = full;
  const players = full.subjects.flatMap((subject) => (subject.player === null ? [] : [subject.player])).filter((uuid, index, all) => all.indexOf(uuid) === index);
  const neighbor = (ref: EntityRef | null, label: string, missing: string) => (
    <li className="flex items-baseline gap-2 border-b border-border px-3.5 py-1.5 text-12">
      <span className="min-w-16 shrink-0 text-subtle">{label}</span>
      {ref?.kind === "request" ? <EntityLink entity={ref} label={time(ref.ts, { millis: true, date: "always" })} mono /> : <span className="text-subtle">{missing}</span>}
    </li>
  );
  return (
    <Section title="Context" flush>
      <ul className="m-0 flex list-none flex-col p-0">
        {neighbor(neighbors.next_same_requester, "Next", "No newer request from this requester within 24 h")}
        {neighbor(neighbors.prev_same_requester, "Previous", "No older request from this requester within 24 h")}
      </ul>
      <div className="flex flex-col gap-1.5 px-3.5 py-2.5 text-12">
        <span className="text-muted">Same player ±5 min: {plural(neighbors.same_player_5m, "other request")}</span>
        {neighbors.same_player_5m > 0 &&
          players.map((uuid) => (
            <span key={uuid} className="inline-flex flex-wrap items-center gap-1.5">
              <EntityLink entity={{ kind: "player", uuid }} />
              <Link to={nearbyPath(uuid, request.ts)} className="text-accent hover:underline">
                requests ±5 min
              </Link>
            </span>
          ))}
      </div>
    </Section>
  );
}

function renamed(player: InvolvedPlayer): boolean {
  return player.name !== null && player.current_name !== null && player.current_name.toLowerCase() !== player.name;
}

function link(ref: EntityRef | null) {
  return ref === null ? NONE : <EntityLink entity={ref} />;
}

function nearbyPath(uuid: string, ts: string): string {
  const at = Date.parse(ts);
  const params = new URLSearchParams({ player: uuid, from: new Date(at - NEARBY_MS).toISOString(), to: new Date(at + NEARBY_MS).toISOString() });
  return `/requests?${params}`;
}
