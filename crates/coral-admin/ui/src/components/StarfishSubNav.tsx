import { NavLink } from "react-router-dom";

const TABS = [
  { to: "/starfish", label: "Users", end: true },
  { to: "/starfish/plugins", label: "Plugins", end: false },
];

export function StarfishSubNav() {
  return (
    <div className="flex gap-1 border-b border-white/8 pb-2">
      {TABS.map((tab) => (
        <NavLink
          key={tab.to}
          to={tab.to}
          end={tab.end}
          className={({ isActive }) =>
            `rounded-md px-3 py-1.5 text-sm ${
              isActive ? "bg-accent/12 font-medium text-accent" : "text-gray-400 hover:bg-white/5 hover:text-gray-200"
            }`
          }
        >
          {tab.label}
        </NavLink>
      ))}
    </div>
  );
}
