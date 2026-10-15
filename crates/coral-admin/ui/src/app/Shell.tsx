import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import { Outlet, ScrollRestoration, useLocation } from "react-router-dom";

import { Toaster } from "../kit/Toaster";
import { cx } from "../lib/cx";
import { useHistoryValue } from "../lib/history-state";
import { useHotkey } from "../lib/hotkeys";
import { Omnibox } from "../omnibox/Omnibox";
import { loadPalette, prefetchOmnibox } from "../omnibox/lazy";
import { useOmniboxState } from "../omnibox/omnibox-store";
import { useBoot } from "./boot";
import { navGroups, pageJumps } from "./nav";
import { NavigationProgress } from "./NavigationProgress";
import { Sidebar } from "./Sidebar";
import { useSidebarRail } from "./sidebar-state";
import { StatusBar } from "./StatusBar";
import { TopBar } from "./TopBar";
import { useGlobalShortcuts } from "./useGlobalShortcuts";

const OMNIBOX_STATE_KEY = "omnibox";
const OMNIBOX_PREFETCH_DELAY_MS = 1_500;

const Palette = lazy(loadPalette);
const ShortcutsDialog = lazy(() => import("./ShortcutsDialog").then((module) => ({ default: module.ShortcutsDialog })));

export function Shell() {
  const boot = useBoot();
  const location = useLocation();
  const [omniboxText, setOmniboxText] = useHistoryValue(OMNIBOX_STATE_KEY);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [nav, setNav] = useState({ open: false, locationKey: location.key });
  const [rail, setRail] = useSidebarRail();
  const { paletteOpen } = useOmniboxState();
  const [paletteMounted, setPaletteMounted] = useState(false);
  if (paletteOpen && !paletteMounted) setPaletteMounted(true);
  const groups = useMemo(() => navGroups(boot.extensions), [boot.extensions]);
  const pages = useMemo(() => pageJumps(groups), [groups]);
  const navOpen = nav.open && nav.locationKey === location.key;
  const closeNav = () => setNav({ open: false, locationKey: location.key });

  useGlobalShortcuts(() => setShortcutsOpen(true));
  useEffect(() => {
    const timer = window.setTimeout(prefetchOmnibox, OMNIBOX_PREFETCH_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, []);
  useHotkey("Escape", closeNav, { enabled: navOpen });

  return (
    <div className="flex min-h-dvh flex-col pb-[calc(var(--spacing-statusbar)+env(safe-area-inset-bottom,0px))]">
      <NavigationProgress />
      <TopBar
        omnibox={<Omnibox text={omniboxText} onTextChange={setOmniboxText} pages={pages} />}
        navOpen={navOpen}
        onOpenNav={() => setNav({ open: true, locationKey: location.key })}
        onShowShortcuts={() => setShortcutsOpen(true)}
      />
      <div className={cx("grid flex-1 grid-cols-[minmax(0,1fr)]", rail ? "lg:grid-cols-[var(--spacing-rail)_minmax(0,1fr)]" : "lg:grid-cols-[var(--spacing-sidebar)_minmax(0,1fr)]")}>
        <Sidebar groups={groups} rail={rail} open={navOpen} onClose={closeNav} onRailChange={setRail} />
        <main id="main" tabIndex={-1} className="flex min-w-0 flex-col gap-5 px-6 pt-5 pb-12 outline-none max-md:px-4">
          <Outlet />
        </main>
      </div>
      <StatusBar />
      <Suspense>
        {paletteMounted && <Palette text={omniboxText} onTextChange={setOmniboxText} pages={pages} />}
        {shortcutsOpen && <ShortcutsDialog open onClose={() => setShortcutsOpen(false)} />}
      </Suspense>
      <Toaster />
      <ScrollRestoration />
    </div>
  );
}
