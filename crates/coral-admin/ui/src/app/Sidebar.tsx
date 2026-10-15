import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { Link, useLocation } from "react-router-dom";

import { cx } from "../lib/cx";
import { isNavActive, type NavGroup, type NavItem } from "./nav";

type SidebarProps = {
  groups: readonly NavGroup[];
  rail: boolean;
  open: boolean;
  onClose: () => void;
  onRailChange: (rail: boolean) => void;
};

export function Sidebar({ groups, rail, open, onClose, onRailChange }: SidebarProps) {
  const { pathname } = useLocation();
  const RailIcon = rail ? PanelLeftOpen : PanelLeftClose;
  return (
    <>
      {open && <div aria-hidden className="fixed inset-0 z-[55] bg-[rgba(4,5,8,0.6)] lg:hidden" onClick={onClose} />}
      <nav
        id="sidebar"
        aria-label="Main"
        className={cx(
          "flex flex-col gap-3.5 overflow-y-auto border-r border-border bg-surface",
          "lg:sticky lg:top-[calc(var(--spacing-topbar)+env(safe-area-inset-top,0px))] lg:h-[calc(100dvh-var(--spacing-topbar)-var(--spacing-statusbar)-env(safe-area-inset-top,0px)-env(safe-area-inset-bottom,0px))] lg:self-start",
          rail ? "lg:px-2 lg:pt-3 lg:pb-5" : "lg:px-2.5 lg:pt-3 lg:pb-5",
          "max-lg:fixed max-lg:inset-y-0 max-lg:left-0 max-lg:z-60 max-lg:w-sheet max-lg:px-2.5 max-lg:pt-[calc(16px+env(safe-area-inset-top,0px))] max-lg:pb-5 max-lg:shadow-[16px_0_40px_rgba(0,0,0,0.4)] max-lg:transition-transform max-lg:duration-160",
          open ? "max-lg:translate-x-0" : "max-lg:invisible max-lg:-translate-x-full",
        )}
      >
        {groups.map((group, index) => (
          <div key={group.title ?? index} className="flex flex-col gap-px">
            {group.title !== null && (
              <div className={cx("px-2 pb-1 text-11 font-semibold tracking-[0.08em] text-subtle uppercase", rail && "lg:hidden")}>{group.title}</div>
            )}
            {rail && index > 0 && <div aria-hidden className="mx-2 mb-1 hidden border-t border-border lg:block" />}
            {group.items.map((entry) => (
              <SidebarLink key={entry.path} entry={entry} rail={rail} active={isNavActive(entry, pathname)} />
            ))}
          </div>
        ))}
        <button
          type="button"
          onClick={() => onRailChange(!rail)}
          aria-label={rail ? "Expand sidebar" : "Collapse sidebar"}
          title={rail ? "Expand sidebar" : "Collapse sidebar"}
          className={cx(
            "mt-auto hidden h-[30px] items-center gap-[9px] rounded-md px-2 text-13 font-medium text-subtle hover:bg-surface-2 hover:text-fg lg:flex",
            rail && "justify-center px-0",
          )}
        >
          <RailIcon className="size-3.5 shrink-0" />
          {!rail && "Collapse"}
        </button>
      </nav>
    </>
  );
}

function SidebarLink({ entry, rail, active }: { entry: NavItem; rail: boolean; active: boolean }) {
  const Icon = entry.icon;
  return (
    <Link
      to={entry.path}
      aria-current={active ? "page" : undefined}
      title={rail ? entry.label : undefined}
      className={cx(
        "flex h-[30px] items-center gap-[9px] rounded-md px-2 text-13 font-medium transition-colors",
        active ? "bg-accent/14 text-fg" : "text-muted hover:bg-surface-2 hover:text-fg",
        rail && "lg:justify-center lg:px-0",
      )}
    >
      <Icon className={cx("size-3.5 shrink-0", active && "text-accent")} />
      <span className={cx("min-w-0 flex-1 truncate", rail && "lg:sr-only")}>{entry.label}</span>
      {entry.badge !== null && (
        <span
          title="Extension page; its title and content come from the server"
          className={cx("rounded-[3px] border border-border-strong px-1 text-[10px] leading-[15px] tracking-[0.06em] text-subtle uppercase", rail && "lg:hidden")}
        >
          {entry.badge}
        </span>
      )}
      {entry.shortcut !== null && (
        <span className={cx("rounded border border-b-2 border-border-strong bg-surface-2 px-[5px] font-mono text-11 leading-4 text-muted opacity-70", rail && "lg:hidden")}>
          {entry.shortcut}
        </span>
      )}
    </Link>
  );
}
