"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

const sections = [
  {
    label: "Getting Started",
    items: [
      { title: "Introduction", href: "/starfish/docs" },
    ],
  },
  {
    label: "Network",
    items: [
      { title: "HTTP", href: "/starfish/docs/http" },
      { title: "Webhooks", href: "/starfish/docs/webhooks" },
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
