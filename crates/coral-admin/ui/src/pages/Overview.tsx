import { Link } from "react-router-dom";
import { useDismissFlag, useOverview } from "../api/overview";
import type { Flag, FlagKind, PluginChangeRow } from "../api/types";
import { ActivityLink } from "../components/ActivityLink";
import { Badge } from "../components/Badge";
import { Identity } from "../components/Identity";
import { Panel } from "../components/Panel";
import { RecentActivity } from "../components/RecentActivity";
import { fmtDate } from "../format";

const KIND_LABELS: Record<FlagKind, string> = {
  budget: "Budget",
  probe: "Auth probing",
  spike: "Traffic spike",
  hypixel_headroom: "Hypixel headroom",
};

const KIND_TONES: Record<FlagKind, "danger" | "warning"> = {
  budget: "warning",
  probe: "danger",
  spike: "warning",
  hypixel_headroom: "danger",
};

export function Overview() {
  const overview = useOverview();
  const dismiss = useDismissFlag();
  const flags = overview.data?.flags ?? [];
  const pluginChanges = overview.data?.recent_plugin_changes ?? [];

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-lg font-semibold text-gray-100">Overview</h1>

      <Panel title="Attention feed">
        {flags.length === 0 ? (
          <div className="text-sm text-gray-500">Nothing needs attention right now.</div>
        ) : (
          <div className="flex flex-col divide-y divide-white/5">
            {flags.map((f) => (
              <FlagRow key={f.flag_key} flag={f} onDismiss={() => dismiss.mutate(f.flag_key)} pending={dismiss.isPending} />
            ))}
          </div>
        )}
      </Panel>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <RecentActivity limit={20} />
        <RecentPluginChanges rows={pluginChanges} />
      </div>
    </div>
  );
}

function FlagRow({ flag, onDismiss, pending }: { flag: Flag; onDismiss: () => void; pending: boolean }) {
  return (
    <div className="flex items-center justify-between gap-3 py-2.5 text-sm first:pt-0 last:pb-0">
      <div className="flex items-center gap-3">
        <Badge label={KIND_LABELS[flag.kind]} tone={KIND_TONES[flag.kind]} />
        {flag.discord_id ? (
          <Identity
            id={flag.discord_id}
            username={flag.discord_username}
            linkTo={flag.member_id ? `/members/${flag.member_id}` : undefined}
          />
        ) : null}
        <span className="text-gray-300">{flag.summary}</span>
      </div>
      <div className="flex items-center gap-3">
        <ActivityLink discordId={flag.discord_id} />
        <button
          disabled={pending}
          onClick={onDismiss}
          className="rounded-md border border-white/10 px-2 py-1 text-xs text-gray-400 hover:bg-white/8 disabled:opacity-40"
        >
          Dismiss 24h
        </button>
      </div>
    </div>
  );
}

function RecentPluginChanges({ rows }: { rows: PluginChangeRow[] }) {
  if (rows.length === 0) return null;
  return (
    <Panel title="Recently disabled / unlisted plugins">
      <table className="w-full text-sm">
        <tbody>
          {rows.map((r, i) => (
            <tr key={i} className="hover:bg-white/4">
              <td className="border-b border-white/5 py-2 whitespace-nowrap text-xs text-gray-500">{fmtDate(r.at)}</td>
              <td className="border-b border-white/5 py-2">
                <Link to={`/plugins/${r.slug}`} className="font-mono text-xs hover:text-accent">
                  {r.slug}
                </Link>
              </td>
              <td className="border-b border-white/5 py-2">
                <Badge label={r.kind} tone={r.kind === "disabled" ? "danger" : "warning"} />
              </td>
              <td className="border-b border-white/5 py-2 text-xs text-gray-400">{r.reason ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Panel>
  );
}
