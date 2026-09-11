import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { useRevokeStarfishSessions, useSetStarfishLicenseStatus, useStarfishUser } from "../api/starfish";
import type {
  StarfishHwidView,
  StarfishInstalledPlugin,
  StarfishOwnedPlugin,
  StarfishSessionView,
  StarfishUserDetailResponse,
} from "../api/types";
import { Badge } from "../components/Badge";
import { ConfirmButton } from "../components/ConfirmButton";
import { Identity } from "../components/Identity";
import { Panel } from "../components/Panel";
import { fmtDate } from "../format";

const LICENSE_TONE: Record<string, "ok" | "danger" | "default"> = {
  active: "ok",
  suspended: "danger",
  inactive: "default",
};

export function StarfishUserDetail() {
  const { id } = useParams();
  const userId = Number(id);
  const navigate = useNavigate();
  const user = useStarfishUser(userId);

  if (user.isLoading) {
    return <div className="text-sm text-gray-500">Loading…</div>;
  }
  if (!user.data) {
    return <div className="text-sm text-gray-500">Starfish user not found.</div>;
  }

  const u = user.data;

  return (
    <div className="flex flex-col gap-6">
      <button onClick={() => navigate("/starfish")} className="w-fit text-sm text-gray-400 hover:text-white">
        ← Back to Starfish
      </button>

      <Panel>
        <div className="flex flex-wrap items-center gap-3">
          <Identity id={u.discord_id} username={u.discord_username} linkTo={u.member_id ? `/members/${u.member_id}` : undefined} />
          <Badge label={u.license_status} tone={LICENSE_TONE[u.license_status] ?? "default"} />
          {u.github_username && <span className="text-xs text-gray-500">GitHub: {u.github_username}</span>}
        </div>
        <div className="mt-2 text-xs text-gray-500">
          Linked {fmtDate(u.created_at)} · updated {fmtDate(u.updated_at)}
        </div>
      </Panel>

      <ActionBar user={u} />

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <HwidsPanel hwids={u.hwids} />
        <SessionsPanel sessions={u.sessions} />
      </div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <InstalledPluginsPanel plugins={u.installed_plugins} />
        <OwnedPluginsPanel plugins={u.owned_plugins} navigate={navigate} />
      </div>
    </div>
  );
}

function ActionBar({ user }: { user: StarfishUserDetailResponse }) {
  const setLicense = useSetStarfishLicenseStatus(user.id);
  const revokeSessions = useRevokeStarfishSessions(user.id);
  const [status, setStatus] = useState(user.license_status);

  return (
    <Panel className="flex flex-wrap items-center gap-2">
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
        disabled={status === user.license_status}
        onConfirm={() => setLicense.mutate(status)}
        pending={setLicense.isPending}
      />
      <ConfirmButton label="Revoke sessions" tone="danger" onConfirm={() => revokeSessions.mutate()} pending={revokeSessions.isPending} />
    </Panel>
  );
}

function HwidsPanel({ hwids }: { hwids: StarfishHwidView[] }) {
  return (
    <Panel title={`HWIDs (${hwids.length})`}>
      {hwids.length === 0 ? (
        <div className="text-sm text-gray-500">No HWIDs registered.</div>
      ) : (
        <div className="flex flex-col divide-y divide-white/5">
          {hwids.map((h) => (
            <div key={h.id} className="flex items-center justify-between py-2 text-sm first:pt-0">
              <span className="flex items-center gap-2">
                <span className="font-mono text-xs text-gray-400">{h.hwid_hash.slice(0, 16)}…</span>
                {h.is_active && <Badge label="Active" tone="ok" />}
              </span>
              <span className="text-xs text-gray-500">
                {h.has_components ? "components recorded" : "no components"} · {fmtDate(h.registered_at)}
              </span>
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}

function SessionsPanel({ sessions }: { sessions: StarfishSessionView[] }) {
  const now = new Date().toISOString();
  return (
    <Panel title={`Recent sessions (${sessions.length})`}>
      {sessions.length === 0 ? (
        <div className="text-sm text-gray-500">No sessions on record.</div>
      ) : (
        <div className="flex flex-col divide-y divide-white/5">
          {sessions.map((s) => (
            <div key={s.id} className="py-2 text-sm first:pt-0">
              <div className="flex items-center justify-between">
                <Badge label={s.expires_at > now ? "Active" : "Expired"} tone={s.expires_at > now ? "ok" : "default"} />
                <span className="text-xs text-gray-500">heartbeat {fmtDate(s.last_heartbeat_at)}</span>
              </div>
              <div className="mt-1 text-xs text-gray-500">
                issued {fmtDate(s.issued_at)} · expires {fmtDate(s.expires_at)}
              </div>
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}

function InstalledPluginsPanel({ plugins }: { plugins: StarfishInstalledPlugin[] }) {
  return (
    <Panel title={`Installed plugins (${plugins.length})`}>
      {plugins.length === 0 ? (
        <div className="text-sm text-gray-500">No plugins installed.</div>
      ) : (
        <div className="flex flex-col divide-y divide-white/5">
          {plugins.map((p) => (
            <div key={p.slug} className="flex items-center justify-between py-2 text-sm first:pt-0">
              <span className="font-mono text-xs text-gray-400">{p.slug}</span>
              <span className="flex items-center gap-2 text-xs text-gray-500">
                v{p.installed_version}
                {p.installed_version !== p.latest_version && <span className="text-warning">latest: v{p.latest_version}</span>}
                {p.disabled && <Badge label="Disabled" tone="danger" />}
              </span>
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}

function OwnedPluginsPanel({ plugins, navigate }: { plugins: StarfishOwnedPlugin[]; navigate: (path: string) => void }) {
  return (
    <Panel title={`Published plugins (${plugins.length})`}>
      {plugins.length === 0 ? (
        <div className="text-sm text-gray-500">No plugins published.</div>
      ) : (
        <div className="flex flex-col divide-y divide-white/5">
          {plugins.map((p) => (
            <div
              key={p.slug}
              onClick={() => navigate(`/starfish/plugins/${p.slug}`)}
              className="flex cursor-pointer items-center justify-between py-2 text-sm first:pt-0 hover:text-accent"
            >
              <span>{p.display_name}</span>
              <div className="flex items-center gap-1">
                {p.official && <Badge label="Official" tone="ok" />}
                {p.unlisted && <Badge label="Unlisted" tone="warning" />}
                {p.disabled && <Badge label="Disabled" tone="danger" />}
              </div>
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}
