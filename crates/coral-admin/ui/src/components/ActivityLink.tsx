import { Link } from "react-router-dom";

type ActivityLinkProps = {
  discordId?: string | null;
  ip?: string | null;
  hours?: number;
  label?: string;
};

export function ActivityLink({ discordId, ip, hours = 24, label = "View activity →" }: ActivityLinkProps) {
  const params = new URLSearchParams();
  if (discordId) {
    params.set("discord_id", discordId);
  } else if (ip) {
    params.set("caller", ip);
  } else {
    return null;
  }
  params.set("hours", String(hours));
  return (
    <Link
      to={`/diagnostics?${params}`}
      className="text-xs text-gray-400 hover:text-white hover:underline"
      onClick={(e) => e.stopPropagation()}
    >
      {label}
    </Link>
  );
}
