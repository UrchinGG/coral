import { useLabel } from "../../api/labels";
import type { AccountCore } from "../../api/types/accounts";
import { Pill } from "../../kit/Badges";
import { CopyField } from "../../kit/Copy";
import { EntityLink } from "../../kit/EntityLink";
import { formatNumber, plural } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { EntityHeader, IdRow } from "../entity/EntityHeader";
import { AccountActions } from "./AccountActions";
import { accessLabel, LICENSE_TONE } from "./vocabulary";

const NO_NAME_INITIAL = "#";

export function AccountHeader({ core }: { core: AccountCore }) {
  const discordId = core.account.discord_id;
  const name = useLabel({ space: "discord", id: discordId });
  const primary = core.minecraft.find((link) => link.relation === "primary");
  const meta = useAccountMeta(core);
  return (
    <EntityHeader
      kind="account"
      round
      icon={(name ?? NO_NAME_INITIAL).slice(0, 1).toUpperCase()}
      title={name ?? <span className="font-mono text-18">{discordId}</span>}
      badges={<AccountPills core={core} />}
      ids={
        <>
          <IdRow label="Discord">
            <CopyField value={discordId} label="Copy Discord id" />
          </IdRow>
          {primary !== undefined && (
            <IdRow label="Minecraft">
              <EntityLink entity={primary.player} />
            </IdRow>
          )}
        </>
      }
      meta={meta}
      actions={core.member === null ? undefined : <AccountActions core={core} member={core.member} />}
    />
  );
}

function AccountPills({ core }: { core: AccountCore }) {
  const { member, strikes, starfish } = core;
  if (member === null) return <Pill tone="warning">No Coral account</Pill>;
  return (
    <>
      {member.is_owner && <Pill tone="accent">dashboard owner</Pill>}
      <Pill tone={member.access_level > 0 ? "accent" : "neutral"}>{accessLabel(member.access_level)}</Pill>
      {member.key_locked ? <Pill tone="danger">key locked</Pill> : <Pill tone="ok">key active</Pill>}
      {member.tagging_disabled && <Pill tone="warning">tagging disabled</Pill>}
      {strikes.active > 0 && <Pill tone="danger">{plural(strikes.active, "active strike")}</Pill>}
      {starfish !== null && <Pill tone={LICENSE_TONE[starfish.license_status] ?? "neutral"}>Starfish {starfish.license_status}</Pill>}
    </>
  );
}

function useAccountMeta(core: AccountCore): string[] {
  const { time, relative } = useTimeFormat();
  const member = core.member;
  if (member === null) return ["No Coral account: activity, lookups and moderation come from what this user did through the bot."];
  return [
    `Member #${member.id}`,
    `Joined ${time(member.join_date, { seconds: false, date: "always" })}`,
    `${formatNumber(core.activity.h24.requests)} requests in 24 h`,
    member.last_request_at === null ? "No request in 14 days" : `Last request ${relative(member.last_request_at)}`,
    `${formatNumber(member.request_count)} lifetime requests admitted with the personal key`,
  ];
}
