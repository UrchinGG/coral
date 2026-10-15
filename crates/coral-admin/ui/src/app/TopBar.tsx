import { Menu } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";

import { IconButton } from "../kit/Button";
import { LivePill } from "./LivePill";
import { PerfHud } from "./PerfHud";
import { TimeZoneToggle } from "./TimeZoneToggle";
import { UserMenu } from "./UserMenu";

type TopBarProps = {
  omnibox: ReactNode;
  navOpen: boolean;
  onOpenNav: () => void;
  onShowShortcuts: () => void;
};

export function TopBar({ omnibox, navOpen, onOpenNav, onShowShortcuts }: TopBarProps) {
  return (
    <header className="sticky top-[env(safe-area-inset-top,0px)] z-40 flex h-topbar items-center gap-2.5 border-b border-border bg-surface pr-4 pl-3 max-md:gap-1.5 max-md:pr-3 max-md:pl-2">
      <IconButton label="Open navigation" aria-controls="sidebar" aria-expanded={navOpen} onClick={onOpenNav} className="lg:hidden">
        <Menu className="size-4" />
      </IconButton>
      <Link to="/" className="flex items-baseline gap-1.5 pr-2.5 pl-1 text-15 font-bold tracking-[-0.01em] whitespace-nowrap max-md:pr-1 max-md:pl-0.5">
        <span aria-hidden className="size-[18px] self-center rounded-[5px] bg-[conic-gradient(from_210deg,#818cf8,#6fd6c3,#f472b6,#818cf8)]" />
        Coral
        <span className="text-12 font-medium text-subtle max-md:hidden">admin</span>
      </Link>
      {omnibox}
      <div className="ml-auto flex items-center gap-2">
        <LivePill />
        <PerfHud />
        <TimeZoneToggle />
        <UserMenu onShowShortcuts={onShowShortcuts} />
      </div>
    </header>
  );
}
