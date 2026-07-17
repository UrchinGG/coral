import { useNavigate } from "react-router-dom";

export function ModerationTabs({ active }: { active: "members" | "players" }) {
  const navigate = useNavigate();
  const tabs: ["members" | "players", string][] = [
    ["members", "Members"],
    ["players", "Players"],
  ];
  return (
    <div className="flex overflow-hidden rounded-md border border-white/8">
      {tabs.map(([value, label]) => (
        <button
          key={value}
          onClick={() => navigate(`/${value}`)}
          className={`px-3 py-1.5 text-xs font-medium ${
            value === active ? "bg-accent/15 text-accent" : "text-gray-400 hover:bg-white/5"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
