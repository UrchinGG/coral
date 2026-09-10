"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

const sections = [
  {
    label: "Getting Started",
    items: [
      { title: "Introduction", href: "/docs" },
    ],
  },
  {
    label: "Communication",
    items: [
      { title: "Chat", href: "/docs/chat" },
      { title: "Commands", href: "/docs/commands" },
      { title: "Display", href: "/docs/display" },
      { title: "Overlay", href: "/docs/overlay" },
      { title: "Text", href: "/docs/text" },
    ],
  },
  {
    label: "Game State",
    items: [
      { title: "Entities", href: "/docs/entities" },
      { title: "Players", href: "/docs/players" },
      { title: "World", href: "/docs/world" },
      { title: "Inventory", href: "/docs/inventory" },
      { title: "Scoreboard", href: "/docs/scoreboard" },
    ],
  },
  {
    label: "Network",
    items: [
      { title: "Network", href: "/docs/network" },
    ],
  },
  {
    label: "Core",
    items: [
      { title: "Events", href: "/docs/events" },
      { title: "Input", href: "/docs/input" },
      { title: "Plugins", href: "/docs/plugins" },
      { title: "Client", href: "/docs/client" },
    ],
  },
];

export function DocsSidebar() {
  const pathname = usePathname();

  return (
    <aside className="w-52 shrink-0 hidden md:block">
      <nav className="sticky top-20 space-y-6">
        {sections.map((section) => (
          <div key={section.label}>
            <div className="text-[11px] text-white/20 uppercase tracking-widest mb-2">{section.label}</div>
            <ul className="space-y-0.5">
              {section.items.map((item) => (
                <li key={item.href}>
                  <Link href={item.href} className={`block text-sm py-1 transition-colors ${pathname === item.href ? "text-white/70" : "text-white/40 hover:text-white/60"}`}>
                    {item.title}
                  </Link>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </nav>
    </aside>
  );
}
