import { useRecentActions } from "../api/actions";
import { fmtDate } from "../format";
import { Identity } from "./Identity";

export function RecentActivity({ limit = 20 }: { limit?: number }) {
  const { data } = useRecentActions(limit);
  const actions = data ?? [];

  if (actions.length === 0) return null;

  return (
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="mb-2 text-sm font-medium text-gray-300">Recent moderation activity</div>
      <table className="w-full text-sm">
        <tbody>
          {actions.map((a) => (
            <tr key={a.id} className="border-t border-white/5">
              <td className="py-1.5 whitespace-nowrap text-xs text-gray-500">{fmtDate(a.ts)}</td>
              <td className="py-1.5">
                <Identity id={a.actor} username={a.actor_username} />
              </td>
              <td className="py-1.5 font-mono text-xs text-gray-300">{a.action}</td>
              <td className="py-1.5 font-mono text-xs text-gray-500">{a.target}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
