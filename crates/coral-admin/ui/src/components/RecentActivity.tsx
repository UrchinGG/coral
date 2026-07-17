import { useRecentActions } from "../api/actions";
import { fmtDate } from "../format";
import { Identity } from "./Identity";
import { Panel } from "./Panel";

export function RecentActivity({ limit = 20 }: { limit?: number }) {
  const { data } = useRecentActions(limit);
  const actions = data ?? [];

  if (actions.length === 0) return null;

  return (
    <Panel title="Recent moderation activity">
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-[11px] font-medium tracking-wide text-gray-500 uppercase">
            <th className="border-b border-white/8 pb-2 font-medium">Time</th>
            <th className="border-b border-white/8 pb-2 font-medium">Actor</th>
            <th className="border-b border-white/8 pb-2 font-medium">Action</th>
            <th className="border-b border-white/8 pb-2 font-medium">Target</th>
          </tr>
        </thead>
        <tbody>
          {actions.map((a) => (
            <tr key={a.id} className="hover:bg-white/4">
              <td className="border-b border-white/5 py-2 whitespace-nowrap text-xs text-gray-500">{fmtDate(a.ts)}</td>
              <td className="border-b border-white/5 py-2">
                <Identity id={a.actor} username={a.actor_username} />
              </td>
              <td className="border-b border-white/5 py-2 font-mono text-xs text-gray-300">{a.action}</td>
              <td className="border-b border-white/5 py-2 font-mono text-xs text-gray-500">{a.target}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Panel>
  );
}
