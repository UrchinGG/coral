import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  useAddStrike,
  useCreateDevKey,
  useDeleteDevKey,
  useLockMember,
  useMember,
  useRegenerateApiKey,
  useRemoveStrike,
  useResetRateLimit,
  useRevokeStarfishSessions,
  useSetAccessLevel,
  useSetDevKeyLocked,
  useSetDevKeyPermissions,
  useSetDevKeyRateLimit,
  useSetLicenseStatus,
  useSetTaggingDisabled,
  useUnlockMember,
} from "../api/members";
import type { MemberDetail as MemberDetailType } from "../api/types";
import { Badge } from "../components/Badge";
import { ConfirmButton } from "../components/ConfirmButton";
import { Identity } from "../components/Identity";
import { MemberActivityPanel } from "../components/MemberActivityPanel";
import { Panel } from "../components/Panel";
import { accessRankLabel, accessRankTone, fmtDate } from "../format";
import { useToast } from "../components/Toast";

const DEV_KEY_PERMISSIONS = [
  { bit: 1, label: "Player Data" },
  { bit: 2, label: "Hypixel" },
  { bit: 4, label: "All Sessions" },
];

export function MemberDetail() {
  const { id } = useParams();
  const memberId = Number(id);
  const navigate = useNavigate();
  const member = useMember(memberId);

  if (member.isLoading) {
    return <div className="text-sm text-gray-500">Loading…</div>;
  }
  if (!member.data) {
    return <div className="text-sm text-gray-500">Member not found.</div>;
  }

  const m = member.data;

  return (
    <div className="flex flex-col gap-6">
      <button onClick={() => navigate("/members")} className="w-fit text-sm text-gray-400 hover:text-white">
        ← Back to members
      </button>

      <Header member={m} />
      <ActionBar member={m} />
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <StandingPanel member={m} />
        <StrikesPanel member={m} />
      </div>
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <AltAccountsPanel member={m} />
        <IpHistoryPanel member={m} />
      </div>
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <DevKeyPanel member={m} />
        <StarfishPanel member={m} />
      </div>
      <AuthoredTagsPanel member={m} />
      <MemberActivityPanel discordId={m.discord_id} />
    </div>
  );
}

function Header({ member }: { member: MemberDetailType }) {
  return (
    <Panel>
      <div className="flex flex-wrap items-center gap-3">
        <Identity id={member.discord_id} username={member.discord_username} />
        {member.uuid && <Identity id={member.uuid} username={member.minecraft_username} kind="minecraft" />}
        {member.is_owner ? (
          <Badge label="Owner" tone="accent" />
        ) : (
          member.access_level > 0 && <Badge label={accessRankLabel(member.access_level)} tone={accessRankTone(member.access_level)} />
        )}
        {member.key_locked && <Badge label="Locked" tone="danger" />}
        {member.tagging_disabled && <Badge label="Tagging disabled" tone="warning" />}
      </div>
      <div className="mt-2 text-xs text-gray-500">
        Joined {fmtDate(member.join_date)} · {member.request_count.toLocaleString()} requests · API key {member.api_key_preview ? `${member.api_key_preview}…` : "none"}
      </div>
    </Panel>
  );
}

function ActionBar({ member }: { member: MemberDetailType }) {
  const { notify } = useToast();
  const lock = useLockMember(member.id);
  const unlock = useUnlockMember(member.id);
  const setAccessLevel = useSetAccessLevel(member.id);
  const setTaggingDisabled = useSetTaggingDisabled(member.id);
  const regenerateKey = useRegenerateApiKey(member.id);
  const resetRateLimit = useResetRateLimit(member.id);
  const [level, setLevel] = useState(String(member.access_level));

  return (
    <Panel className="flex flex-wrap items-center gap-2">
      {member.key_locked ? (
        <ConfirmButton label="Unlock key" onConfirm={() => unlock.mutate()} pending={unlock.isPending} />
      ) : (
        <ConfirmButton label="Lock key" tone="danger" onConfirm={() => lock.mutate()} pending={lock.isPending} />
      )}
      <span className="flex items-center gap-1">
        <select
          value={level}
          onChange={(e) => setLevel(e.target.value)}
          className="rounded-md border border-white/10 bg-black/30 px-2 py-1 text-xs"
        >
          {[0, 2, 3, 4, 5].map((lvl) => (
            <option key={lvl} value={lvl}>
              {accessRankLabel(lvl)}
            </option>
          ))}
        </select>
        <ConfirmButton
          label="Set access"
          onConfirm={() => setAccessLevel.mutate(Number(level))}
          pending={setAccessLevel.isPending}
          disabled={Number(level) === member.access_level}
        />
      </span>
      <ConfirmButton
        label={member.tagging_disabled ? "Enable tagging" : "Disable tagging"}
        tone={member.tagging_disabled ? "default" : "danger"}
        onConfirm={() => setTaggingDisabled.mutate(!member.tagging_disabled)}
        pending={setTaggingDisabled.isPending}
      />
      <ConfirmButton
        label="Regenerate API key"
        tone="danger"
        onConfirm={() =>
          regenerateKey.mutate(undefined, {
            onSuccess: (data) => notify(`New key: ${data.api_key}`, "success"),
          })
        }
        pending={regenerateKey.isPending}
      />
      <ConfirmButton
        label="Reset rate-limit budget"
        onConfirm={() => resetRateLimit.mutate()}
        pending={resetRateLimit.isPending}
      />
    </Panel>
  );
}

function StandingPanel({ member }: { member: MemberDetailType }) {
  const s = member.standing;
  return (
    <Panel title="Standing">
      <div className="flex flex-col gap-3 text-sm">
        <div>
          <Badge label={s.can_vote ? "Can vote" : "Cannot vote"} tone={s.can_vote ? "ok" : "default"} />
          <div className="mt-1 text-xs text-gray-400">{s.vote_reason}</div>
        </div>
        <div>
          <Badge label={s.can_tag ? "Can tag" : "Cannot tag"} tone={s.can_tag ? "ok" : "default"} />
          <div className="mt-1 text-xs text-gray-400">{s.tag_reason}</div>
        </div>
        <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs text-gray-500">
          <span>Accepted tags: {s.accepted_tags}</span>
          <span>Rejected tags: {s.rejected_tags}</span>
          <span>Accurate verdicts: {s.accurate_verdicts}</span>
          <span>Incorrect verdicts: {s.incorrect_verdicts}</span>
          <span>Bonus verdicts: {s.bonus_verdicts}</span>
          <span>Effective level: {accessRankLabel(s.effective_level)}</span>
        </div>
      </div>
    </Panel>
  );
}

function StrikesPanel({ member }: { member: MemberDetailType }) {
  const addStrike = useAddStrike(member.id);
  const removeStrike = useRemoveStrike(member.id);
  const [reason, setReason] = useState("");

  return (
    <Panel title={`Strikes (${member.strikes.length})`}>
      <div className="flex flex-col divide-y divide-white/5">
        {member.strikes.map((strike, i) => (
          <div key={i} className="flex items-start justify-between gap-2 py-2 text-sm first:pt-0">
            <div>
              <div>{strike.reason}</div>
              <div className="text-xs text-gray-500">
                {fmtDate(strike.timestamp)} · struck by <Identity id={strike.struck_by} />
              </div>
            </div>
            <ConfirmButton
              label="Remove"
              tone="danger"
              onConfirm={() => removeStrike.mutate(i)}
              pending={removeStrike.isPending}
            />
          </div>
        ))}
        {member.strikes.length === 0 && <div className="text-sm text-gray-500">No strikes on record.</div>}
      </div>
      <div className="mt-3 flex gap-2">
        <input
          className="flex-1 rounded-md border border-white/10 bg-black/30 px-2 py-1 text-sm"
          placeholder="Strike reason…"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
        <ConfirmButton
          label="Add strike"
          tone="danger"
          disabled={!reason.trim()}
          onConfirm={() => {
            addStrike.mutate(reason, { onSuccess: () => setReason("") });
          }}
          pending={addStrike.isPending}
        />
      </div>
    </Panel>
  );
}

function AltAccountsPanel({ member }: { member: MemberDetailType }) {
  return (
    <Panel title={`Alt accounts (${member.alt_accounts.length})`}>
      {member.alt_accounts.length === 0 ? (
        <div className="text-sm text-gray-500">None linked.</div>
      ) : (
        <div className="flex flex-col gap-1.5">
          {member.alt_accounts.map((a) => (
            <div key={a.uuid} className="flex items-center justify-between text-sm">
              <Identity id={a.uuid} username={a.minecraft_username} kind="minecraft" />
              <span className="text-xs text-gray-500">added {fmtDate(a.added_at)}</span>
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}

function IpHistoryPanel({ member }: { member: MemberDetailType }) {
  return (
    <Panel title={`IP history (${member.ips.length})`}>
      {member.ips.length === 0 ? (
        <div className="text-sm text-gray-500">No IPs recorded.</div>
      ) : (
        <div className="flex flex-col gap-1.5">
          {member.ips.map((ip) => (
            <div key={ip.ip_address} className="flex items-center justify-between text-sm">
              <span className="font-mono text-xs">{ip.ip_address}</span>
              <span className="text-xs text-gray-500">
                {fmtDate(ip.first_seen)} – {fmtDate(ip.last_seen)}
              </span>
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}

function DevKeyPanel({ member }: { member: MemberDetailType }) {
  const createDevKey = useCreateDevKey(member.id);
  const deleteDevKey = useDeleteDevKey(member.id);
  const setLocked = useSetDevKeyLocked(member.id);
  const setRateLimit = useSetDevKeyRateLimit(member.id);
  const setPermissions = useSetDevKeyPermissions(member.id);
  const { notify } = useToast();
  const [label, setLabel] = useState("");
  const [rateLimitDraft, setRateLimitDraft] = useState(String(member.dev_key?.rate_limit ?? 100));

  if (!member.dev_key) {
    return (
      <Panel title="Developer key">
        <div className="mb-2 text-sm text-gray-500">No developer key issued.</div>
        <div className="flex gap-2">
          <input
            className="flex-1 rounded-md border border-white/10 bg-black/30 px-2 py-1 text-sm"
            placeholder="Label…"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
          <ConfirmButton
            label="Create"
            disabled={!label.trim()}
            onConfirm={() =>
              createDevKey.mutate(
                { label, permissions: 1, rate_limit: 100 },
                { onSuccess: (data) => notify(`New dev key: ${data.api_key}`, "success") },
              )
            }
            pending={createDevKey.isPending}
          />
        </div>
      </Panel>
    );
  }

  const key = member.dev_key;
  return (
    <Panel title={`Developer key — ${key.label}`} action={key.locked ? <Badge label="Locked" tone="danger" /> : undefined}>
      <div className="mb-2 text-xs text-gray-500">{key.request_count.toLocaleString()} requests</div>
      <div className="mb-3 flex flex-wrap gap-1">
        {DEV_KEY_PERMISSIONS.map((p) => {
          const has = (key.permissions & p.bit) !== 0;
          return (
            <button
              key={p.bit}
              onClick={() => setPermissions.mutate(has ? key.permissions & ~p.bit : key.permissions | p.bit)}
              className={`rounded-full px-2 py-0.5 text-xs ${has ? "bg-ok/15 text-ok" : "bg-white/8 text-gray-400"}`}
            >
              {p.label}
            </button>
          );
        })}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <ConfirmButton
          label={key.locked ? "Unlock" : "Lock"}
          tone={key.locked ? "default" : "danger"}
          onConfirm={() => setLocked.mutate(!key.locked)}
          pending={setLocked.isPending}
        />
        <span className="flex items-center gap-1">
          <input
            type="number"
            className="w-20 rounded-md border border-white/10 bg-black/30 px-2 py-1 text-xs"
            value={rateLimitDraft}
            onChange={(e) => setRateLimitDraft(e.target.value)}
          />
          <ConfirmButton
            label="Set limit"
            onConfirm={() => setRateLimit.mutate(Number(rateLimitDraft))}
            pending={setRateLimit.isPending}
          />
        </span>
        <ConfirmButton label="Delete key" tone="danger" onConfirm={() => deleteDevKey.mutate()} pending={deleteDevKey.isPending} />
      </div>
    </Panel>
  );
}

function StarfishPanel({ member }: { member: MemberDetailType }) {
  const setLicense = useSetLicenseStatus(member.id);
  const revokeSessions = useRevokeStarfishSessions(member.id);
  const [status, setStatus] = useState(member.starfish?.license_status ?? "inactive");

  if (!member.starfish) {
    return (
      <Panel title="Starfish">
        <div className="text-sm text-gray-500">No Starfish account.</div>
      </Panel>
    );
  }

  return (
    <Panel title="Starfish">
      <div className="mb-3 flex items-center gap-2 text-sm">
        <Badge label={member.starfish.license_status} tone={member.starfish.license_status === "active" ? "ok" : "default"} />
        {member.starfish.has_active_session && <Badge label="Active session" tone="ok" />}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <select
          value={status}
          onChange={(e) => setStatus(e.target.value)}
          className="rounded-md border border-white/10 bg-black/30 px-2 py-1 text-xs"
        >
          <option value="inactive">inactive</option>
          <option value="active">active</option>
          <option value="suspended">suspended</option>
        </select>
        <ConfirmButton
          label="Set status"
          disabled={status === member.starfish.license_status}
          onConfirm={() => setLicense.mutate(status)}
          pending={setLicense.isPending}
        />
        <ConfirmButton
          label="Revoke sessions"
          tone="danger"
          onConfirm={() => revokeSessions.mutate()}
          pending={revokeSessions.isPending}
        />
      </div>
    </Panel>
  );
}

function AuthoredTagsPanel({ member }: { member: MemberDetailType }) {
  return (
    <Panel title={`Tag actions authored (${member.authored_tags.length})`}>
      {member.authored_tags.length === 0 ? (
        <div className="text-sm text-gray-500">This member has not authored any tag actions.</div>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-[11px] font-medium tracking-wide text-gray-500 uppercase">
              <th className="border-b border-white/8 pb-2 font-medium">Target</th>
              <th className="border-b border-white/8 pb-2 font-medium">Action</th>
              <th className="border-b border-white/8 pb-2 font-medium">Reason</th>
              <th className="border-b border-white/8 pb-2 font-medium">When</th>
            </tr>
          </thead>
          <tbody>
            {member.authored_tags.map((t) => (
              <tr key={t.id} className="hover:bg-white/4">
                <td className="border-b border-white/5 py-2">
                  <Identity id={t.uuid} username={t.minecraft_username} kind="minecraft" />
                </td>
                <td className="border-b border-white/5 py-2">
                  <Badge label={t.kind === "tag_set" ? `+${t.tag_type}` : `-${t.tag_type}`} tone={t.kind === "tag_set" ? "warning" : "default"} />
                </td>
                <td className="border-b border-white/5 py-2 text-xs text-gray-400">{t.reason ?? "—"}</td>
                <td className="border-b border-white/5 py-2 text-xs text-gray-500">{fmtDate(t.ts)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Panel>
  );
}
