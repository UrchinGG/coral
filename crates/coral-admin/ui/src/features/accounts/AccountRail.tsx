import { useQuery } from "@tanstack/react-query";
import { KeyRound } from "lucide-react";
import { useState } from "react";

import { accountActions, keyHistoryQuery, limitsQuery, linkHistoryQuery } from "../../api/accounts";
import { KEY_PREFIX_LENGTH } from "../../api/apiKeys";
import type { AccountCore, Budget, KeyRecord, MemberView, StandingView, StrikeRow } from "../../api/types/accounts";
import type { KeyKind, KeyRef } from "../../api/types/refs";
import { Badge, Pill } from "../../kit/Badges";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { CopyField } from "../../kit/Copy";
import { EntityLink } from "../../kit/EntityLink";
import { KeyValue } from "../../kit/KeyValue";
import { Meter } from "../../kit/Meter";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { EmptyState } from "../../kit/States";
import { plural } from "../../lib/format";
import { refIdentity } from "../../lib/refs";
import { useTimeFormat } from "../../lib/time";
import { dashUuid } from "../../lib/uuid";
import { DeveloperKey } from "./DeveloperKey";
import { useAccountMutation } from "./useAccountMutation";
import { LIMITER_LABEL, STRIKE_TONE } from "./vocabulary";

export function AccountRail({ core }: { core: AccountCore }) {
  const discordId = core.account.discord_id;
  return (
    <aside className="flex min-w-0 flex-col gap-4">
      <MinecraftAccounts core={core} />
      {core.member !== null && <ApiKey core={core} member={core.member} />}
      {core.member !== null && <DeveloperKey discordId={discordId} devKey={core.keys.developer} />}
      {core.member !== null && <KeyHistory discordId={discordId} />}
      <Limits discordId={discordId} />
      {core.standing !== null && <Standing discordId={discordId} standing={core.standing} strikes={core.strikes.items} />}
    </aside>
  );
}

function MinecraftAccounts({ core }: { core: AccountCore }) {
  const { time } = useTimeFormat();
  return (
    <Section title="Minecraft accounts" count={core.minecraft.length}>
      <div className="flex flex-col gap-3">
        {core.minecraft.length === 0 && <span className="text-12 text-subtle">No linked Minecraft account</span>}
        {core.minecraft.map((link) => (
          <div key={`${link.relation}:${refIdentity(link.player)}`} className="flex flex-col gap-1">
            <div className="flex flex-wrap items-center gap-1.5">
              <EntityLink entity={link.player} />
              <Badge>{link.relation}</Badge>
              {link.linked_at !== null && <span className="text-11 text-subtle">since {time(link.linked_at, { seconds: false, date: "always" })}</span>}
            </div>
            <UuidForms player={link.player.kind === "player" ? link.player.uuid : null} />
            {link.also_primary_of.length > 0 && (
              <span className="flex flex-wrap items-center gap-1 text-11 text-warning">
                Also the primary account of
                {link.also_primary_of.map((account) => (
                  <EntityLink key={account.discord_id} entity={account} />
                ))}
              </span>
            )}
          </div>
        ))}
        {core.former_minecraft > 0 && <FormerLinks discordId={core.account.discord_id} count={core.former_minecraft} />}
      </div>
    </Section>
  );
}

function FormerLinks({ discordId, count }: { discordId: string; count: number }) {
  const [open, setOpen] = useState(false);
  const history = useQuery({ ...linkHistoryQuery(discordId), enabled: open });
  const { time } = useTimeFormat();
  return (
    <details className="border-t border-border pt-2" onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary className="cursor-pointer text-12 text-muted">Former ({count})</summary>
      <div className="mt-2 flex flex-col gap-2.5">
        <QueryContent query={history}>
          {(links) =>
            links
              .filter((link) => link.unlinked_at !== null)
              .map((link) => (
                <div key={`${link.uuid}:${link.unlinked_at}`} className="flex flex-col gap-1">
                  <div className="flex flex-wrap items-center gap-1.5">
                    <EntityLink entity={{ kind: "player", uuid: link.uuid }} />
                    <Badge>{link.relation}</Badge>
                  </div>
                  <span className="text-11 text-subtle">
                    {link.linked_at === null ? "linked before history began" : time(link.linked_at, { seconds: false, date: "always" })} →{" "}
                    {link.unlinked_at === null ? "now" : time(link.unlinked_at, { seconds: false, date: "always" })}
                  </span>
                  <UuidForms player={link.uuid} />
                </div>
              ))
          }
        </QueryContent>
      </div>
    </details>
  );
}

function UuidForms({ player }: { player: string | null }) {
  if (player === null) return null;
  return (
    <div className="flex flex-col gap-0.5">
      <CopyField value={player} label="Copy undashed UUID" />
      <CopyField value={dashUuid(player)} label="Copy dashed UUID" />
    </div>
  );
}

function ApiKey({ core, member }: { core: AccountCore; member: MemberView }) {
  const personal = core.keys.personal;
  return (
    <Section title="API key" icon={KeyRound}>
      {personal === null ? (
        <span className="text-12 text-subtle">No personal key</span>
      ) : (
        <div className="flex flex-col gap-1.5">
          <CopyField value={personal} label="Copy API key" secret />
          <div className="flex flex-wrap items-center gap-2 text-11 text-subtle">
            {member.key_locked ? <Pill tone="danger">locked</Pill> : <Pill tone="ok">active</Pill>}
            <EntityLink entity={keyRef(null, personal, "personal")} label="Requests with this key" plain />
          </div>
        </div>
      )}
    </Section>
  );
}

function KeyHistory({ discordId }: { discordId: string }) {
  const history = useQuery(keyHistoryQuery(discordId));
  return (
    <Section title="Key history" count={history.data?.length} fetching={history.isFetching && history.data !== undefined}>
      <QueryContent query={history} isEmpty={(records) => records.length === 0} empty="No keys recorded">
        {(records) => (
          <div className="flex flex-col gap-3">
            {records.map((record) => (
              <KeyHistoryEntry key={record.key_id} record={record} />
            ))}
          </div>
        )}
      </QueryContent>
    </Section>
  );
}

function KeyHistoryEntry({ record }: { record: KeyRecord }) {
  const { time } = useTimeFormat();
  const stamp = (iso: string | null, fallback: string) => (iso === null ? fallback : time(iso, { seconds: false, date: "always" }));
  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-center gap-1.5">
        <EntityLink entity={keyRef(record.retired_at === null ? null : record.key_id, record.key, record.kind === "developer" ? "developer" : "personal")} />
        <Badge variant="key">{record.kind}</Badge>
        {record.retired_at === null ? <Pill tone="ok">current</Pill> : <Pill>retired</Pill>}
      </div>
      <CopyField value={record.key} label={`Copy ${record.retired_at === null ? "current" : "retired"} key`} secret />
      <span className="text-11 text-subtle">
        created {stamp(record.created_at, "before history began")}
        {record.retired_at !== null && ` · retired ${stamp(record.retired_at, "")}`}
      </span>
    </div>
  );
}

function Limits({ discordId }: { discordId: string }) {
  const limits = useQuery(limitsQuery(discordId));
  return (
    <Section title="Limits" fetching={limits.isFetching && limits.data !== undefined} actions={<span className="text-11 text-subtle">polled every 10 s</span>}>
      <QueryContent query={limits}>
        {(budgets) => (
          <div className="flex flex-col gap-3">
            {budgets.map((budget) => (
              <LimitMeter key={`${budget.limiter}:${budget.player}`} budget={budget} />
            ))}
          </div>
        )}
      </QueryContent>
    </Section>
  );
}

function LimitMeter({ budget }: { budget: Budget }) {
  const vocabulary = LIMITER_LABEL[budget.limiter];
  if (budget.limit === null) {
    return (
      <div className="flex justify-between gap-2 text-12">
        <span>{vocabulary.label}</span>
        <span className="font-mono text-muted">{budget.used} · unlimited</span>
      </div>
    );
  }
  const label = budget.player === null ? vocabulary.label : <>{vocabulary.label} · <EntityLink entity={{ kind: "player", uuid: budget.player }} /></>;
  return <Meter used={budget.used} limit={budget.limit} label={label} unit={vocabulary.unit} />;
}

function Standing({ discordId, standing, strikes }: { discordId: string; standing: StandingView; strikes: readonly StrikeRow[] }) {
  return (
    <Section title="Standing">
      <div className="flex flex-col gap-3">
        <KeyValue
          items={[
            { label: "Votes", value: standing.can_vote ? <Pill tone="ok">can vote</Pill> : <span className="text-muted">{standing.vote_reason}</span> },
            { label: "Tags", value: standing.can_tag ? <Pill tone="ok">can tag</Pill> : <span className="text-muted">{standing.tag_reason}</span> },
            { label: "Effective level", value: standing.effective_level },
            { label: "Submissions", value: `${standing.accepted_tags} accepted · ${standing.rejected_tags} rejected` },
            { label: "Verdicts", value: `${standing.accurate_verdicts} accurate · ${standing.incorrect_verdicts} incorrect · ${standing.bonus_verdicts} bonus` },
            { label: "Strikes", value: plural(standing.strike_count, "active strike") },
          ]}
        />
        {strikes.length === 0 ? <EmptyState>No strikes</EmptyState> : strikes.map((strike) => <StrikeEntry key={strike.id} discordId={discordId} strike={strike} />)}
      </div>
    </Section>
  );
}

function StrikeEntry({ discordId, strike }: { discordId: string; strike: StrikeRow }) {
  const { time } = useTimeFormat();
  const revoke = useAccountMutation(discordId, {
    run: () => accountActions.revokeStrike(discordId, strike.id),
    success: () => `Strike #${strike.id} revoked.`,
  });
  return (
    <div className="flex flex-col gap-1 border-t border-border pt-2 text-12">
      <div className="flex flex-wrap items-center gap-1.5">
        <Pill tone={STRIKE_TONE[strike.status]}>{strike.status}</Pill>
        <span className="wrap-anywhere">{strike.reason}</span>
      </div>
      <span className="flex flex-wrap items-center gap-1 text-11 text-subtle">
        by <EntityLink entity={strike.struck_by} /> · {time(strike.issued_at, { seconds: false, date: "always" })}
        {strike.expires_at !== null && ` · expires ${time(strike.expires_at, { seconds: false, date: "always" })}`}
        {strike.revoked_by !== null && (
          <>
            · revoked by <EntityLink entity={strike.revoked_by} />
          </>
        )}
      </span>
      {strike.status === "active" && (
        <div>
          <ConfirmButton size="sm" confirmLabel="Revoke strike" onConfirm={() => revoke.mutate()} pending={revoke.isPending}>
            Revoke
          </ConfirmButton>
        </div>
      )}
    </div>
  );
}

function keyRef(keyId: number | null, key: string, keyKind: KeyKind): KeyRef {
  return { kind: "key", key_id: keyId, prefix: key.slice(0, KEY_PREFIX_LENGTH), key_kind: keyKind };
}
