import { Link } from "react-router-dom";
import { useDismissFlag, useOverview } from "../api/overview";
import type { Flag, FlagKind, PluginChangeRow } from "../api/types";
import { ActivityLink } from "../components/ActivityLink";
import { Badge } from "../components/Badge";
import { Identity } from "../components/Identity";
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
    <div className="flex flex-col gap-5">
      <h1 className="text-lg font-semibold">Overview</h1>

      <div className="rounded-lg border border-white/10 bg-white/5 p-4">
        <div className="mb-2 text-sm font-medium text-gray-300">Attention feed</div>
        {flags.length === 0 ? (
          <div className="text-sm text-gray-500">Nothing needs attention right now.</div>
        ) : (
          <div className="flex flex-col gap-2">
            {flags.map((f) => (
              <FlagRow key={f.flag_key} flag={f} onDismiss={() => dismiss.mutate(f.flag_key)} pending={dismiss.isPending} />
            ))}
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        <RecentActivity limit={20} />
        <RecentPluginChanges rows={pluginChanges} />
      </div>
    </div>
  );
}

function FlagRow({ flag, onDismiss, pending }: { flag: Flag; onDismiss: () => void; pending: boolean }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded border border-white/5 p-2 text-sm">
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
          className="rounded border border-white/10 px-2 py-1 text-xs text-gray-400 hover:bg-white/10 disabled:opacity-40"
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
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="mb-2 text-sm font-medium text-gray-300">Recently disabled / unlisted plugins</div>
      <table className="w-full text-sm">
        <tbody>
          {rows.map((r, i) => (
            <tr key={i} className="border-t border-white/5">
              <td className="py-1.5 whitespace-nowrap text-xs text-gray-500">{fmtDate(r.at)}</td>
              <td className="py-1.5">
                <Link to={`/plugins/${r.slug}`} className="font-mono text-xs hover:underline">
                  {r.slug}
                </Link>
              </td>
              <td className="py-1.5">
                <Badge label={r.kind} tone={r.kind === "disabled" ? "danger" : "warning"} />
              </td>
              <td className="py-1.5 text-xs text-gray-400">{r.reason ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
