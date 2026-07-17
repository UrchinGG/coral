import { useNavigate } from "react-router-dom";

export function ModerationTabs({ active }: { active: "members" | "players" }) {
  const navigate = useNavigate();
  const tabs: ["members" | "players", string][] = [
    ["members", "Members"],
    ["players", "Players"],
  ];
  return (
    <div className="flex overflow-hidden rounded border border-white/10">
      {tabs.map(([value, label]) => (
        <button
          key={value}
          onClick={() => navigate(`/${value}`)}
          className={`px-3 py-1.5 text-sm ${
            value === active ? "bg-white/15 text-white" : "text-gray-400 hover:bg-white/5"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
