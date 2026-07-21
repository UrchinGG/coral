import { NavLink, Outlet, useLocation } from "react-router-dom";

const NAV_ITEMS = [
  { to: "/", label: "Overview", end: true, extraMatch: [] as string[] },
  { to: "/diagnostics", label: "API Diagnostics", end: false, extraMatch: [] },
  { to: "/members", label: "Members & Moderation", end: false, extraMatch: ["/players"] },
  { to: "/servers", label: "Server Config", end: false, extraMatch: [] },
  { to: "/plugins", label: "Plugins", end: false, extraMatch: [] },
  { to: "/data", label: "Data", end: false, extraMatch: [] },
];

export function AppShell() {
  const location = useLocation();

  return (
    <div className="flex min-h-screen bg-[#0a0c11] text-gray-200">
      <aside className="flex w-56 flex-shrink-0 flex-col border-r border-white/8 p-4">
        <div className="mb-8 px-2 text-sm font-semibold tracking-wide text-gray-100">Coral Admin</div>
        <nav className="flex flex-col gap-0.5">
          {NAV_ITEMS.map((item) => {
            const active = item.extraMatch.some((path) => location.pathname.startsWith(path));
            return (
              <NavLink
                key={item.to}
                to={item.to}
                end={item.end}
                className={({ isActive }) =>
                  `rounded-md px-3 py-2 text-sm transition-colors ${
                    isActive || active
                      ? "bg-accent/12 font-medium text-accent"
                      : "text-gray-400 hover:bg-white/5 hover:text-gray-200"
                  }`
                }
              >
                {item.label}
              </NavLink>
            );
          })}
        </nav>
        <div className="mt-auto">
          <a href="/auth/logout" className="block rounded-md px-3 py-2 text-sm text-gray-500 hover:bg-white/5 hover:text-gray-300">
            Log out
          </a>
        </div>
      </aside>
      <main className="min-w-0 flex-1 overflow-auto p-8">
        <Outlet />
      </main>
    </div>
  );
}
