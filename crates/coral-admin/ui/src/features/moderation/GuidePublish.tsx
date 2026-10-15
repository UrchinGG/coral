import { useQuery } from "@tanstack/react-query";

import { keys } from "../../api/keys";
import { publishGuide, savePingRoles } from "../../api/moderation";
import { useAction } from "../../api/mutate";
import { directoryQuery, populationQuery } from "../../api/server";
import type { Guide, PingRoles } from "../../api/types/platform";
import { Pill } from "../../kit/Badges";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { EntityLink } from "../../kit/EntityLink";
import { Section } from "../../kit/Section";
import { ErrorState } from "../../kit/States";
import { formatNumber } from "../../lib/format";
import { roleChoices } from "../server/choices";
import { DiscordPicker } from "../server/DiscordPicker";
import { When } from "../platform/When";

type PingRoleField = keyof PingRoles;

const PING_ROLE_ROWS: readonly { field: PingRoleField; label: string; description: string }[] = [
  { field: "review_role_id", label: "All reviews", description: "Pinged for every new tag submission." },
  { field: "dispute_role_id", label: "Disputes only", description: "Pinged when votes disagree and a moderator call is needed." },
  { field: "confirm_role_id", label: "Confirm ping role", description: "Pinged when a review is accepted with media and its tag can be confirmed." },
];

export function PublishPanel({ guide, dirty }: { guide: Guide; dirty: boolean }) {
  const directory = useQuery(directoryQuery());
  const publish = useAction<void, void>({
    run: publishGuide,
    pending: () => (guide.status.posted ? "Editing the posted guide…" : "Posting the guide…"),
    done: () => "Guide posted and pinned",
    invalidates: [keys.moderation.guide()],
  });
  const { status } = guide;
  const forum = directory.data?.channels.find((channel) => channel.id === status.forum_channel_id);

  return (
    <Section title="Publish">
      <div className="flex flex-col gap-2 text-12">
        <p className="m-0 text-muted">Posts a pinned thread in the review forum. If one is already posted, this edits it in place.</p>
        {status.posted ? (
          <>
            <span className="flex flex-wrap items-center gap-2">
              <Pill tone="ok">Posted</Pill>
              <span>in {forum === undefined ? <span className="font-mono">{status.forum_channel_id ?? "an unconfigured forum"}</span> : `#${forum.name}`}</span>
            </span>
            <span className="flex flex-wrap items-center gap-1.5 text-muted">
              Posted <When at={status.posted_at} />
              {status.posted_by !== null && (
                <>
                  by <EntityLink entity={status.posted_by} />
                </>
              )}
            </span>
            {status.up_to_date ? <Pill tone="ok">Up to date</Pill> : <UnpublishedNotice />}
          </>
        ) : (
          <span className="text-muted">The guide has not been posted yet.</span>
        )}
        {dirty && <span className="text-warning">You have unsaved edits; posting publishes the saved content, so save first.</span>}
        <span>
          <ConfirmButton confirmLabel={status.posted ? "Edit posted thread" : "Post pinned thread"} pending={publish.isPending} onConfirm={() => publish.mutate()}>
            {status.posted ? "Update posted guide" : "Post guide"}
          </ConfirmButton>
        </span>
      </div>
    </Section>
  );
}

export function PingRolesPanel({ roles }: { roles: PingRoles }) {
  const directory = useQuery(directoryQuery());
  const population = useQuery(populationQuery());
  const save = useAction({
    run: savePingRoles,
    pending: () => "Saving the ping roles…",
    done: () => "Ping roles updated",
    invalidates: [keys.moderation.guide()],
  });
  const choices = roleChoices(directory.data?.roles, { manager: "bot" });

  return (
    <Section title="Review ping roles">
      <div className="flex flex-col gap-4">
        <p className="m-0 text-12 text-muted">Members opt in and out themselves with the guide's button in Discord. Coral pings these roles from review threads.</p>
        {directory.error !== null && <ErrorState error={directory.error} title="Role names unavailable" onRetry={() => void directory.refetch()} retrying={directory.isFetching} />}
        {population.error !== null && <ErrorState error={population.error} title="Opt-in counts unavailable" onRetry={() => void population.refetch()} retrying={population.isFetching} />}
        {PING_ROLE_ROWS.map((row) => {
          const roleId = roles[row.field];
          const optIns = roleId === null ? null : population.data?.role_members[roleId];
          return (
            <div key={row.field} className="flex flex-col gap-1">
              <span className="flex flex-wrap items-center gap-2">
                <span className="text-13 font-medium">{row.label}</span>
                {optIns !== null && optIns !== undefined && <span className="text-12 text-subtle">{formatNumber(optIns)} opted in</span>}
              </span>
              <span className="text-12 text-muted">{row.description}</span>
              <DiscordPicker label={row.label} choices={choices} value={roleId} onChange={(id) => save.mutate({ ...roles, [row.field]: id })} placeholder="Select a role" prefix="@" />
            </div>
          );
        })}
        {population.data !== undefined && (
          <span className="text-11 text-subtle">
            Opt-in counts as of <When at={population.data.as_of} />
          </span>
        )}
      </div>
    </Section>
  );
}

function UnpublishedNotice() {
  return (
    <span className="flex flex-col gap-1">
      <Pill tone="warning">Unpublished changes</Pill>
      <span className="text-muted">The saved guide differs from the posted thread. Update to publish.</span>
    </span>
  );
}
