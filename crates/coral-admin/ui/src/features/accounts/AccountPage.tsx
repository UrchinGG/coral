import { useQuery } from "@tanstack/react-query";
import { lazy, Suspense, useEffect, type ComponentType } from "react";
import { useParams } from "react-router-dom";

import { accountCoreQuery } from "../../api/accounts";
import type { AccountCore } from "../../api/types/accounts";
import { QueryContent } from "../../kit/QueryContent";
import { SkeletonLines } from "../../kit/States";
import { Tabs } from "../../kit/Tabs";
import { useUrlEnum } from "../../lib/url-state";
import { AccountHeader } from "./AccountHeader";
import { AccountRail } from "./AccountRail";
import { ACCOUNT_TABS, DEFAULT_TAB, type AccountTab } from "./vocabulary";

const TAB_LABEL: Record<AccountTab, string> = {
  activity: "Activity",
  network: "Network",
  lookups: "Lookups",
  moderation: "Moderation",
  starfish: "Starfish",
  raw: "Raw",
};

type TabPanel = ComponentType<{ core: AccountCore }>;

const TAB_MODULES: Record<AccountTab, () => Promise<{ default: TabPanel }>> = {
  activity: () => import("./tabs/ActivityTab").then((module) => ({ default: module.ActivityTab })),
  network: () => import("./tabs/NetworkTab").then((module) => ({ default: module.NetworkTab })),
  lookups: () => import("./tabs/LookupsTab").then((module) => ({ default: module.LookupsTab })),
  moderation: () => import("./tabs/ModerationTab").then((module) => ({ default: module.ModerationTab })),
  starfish: () => import("./tabs/StarfishTab").then((module) => ({ default: module.StarfishTab })),
  raw: () => import("./tabs/RawTab").then((module) => ({ default: module.RawTab })),
};

const TAB_PANELS: Record<AccountTab, TabPanel> = {
  activity: lazy(TAB_MODULES.activity),
  network: lazy(TAB_MODULES.network),
  lookups: lazy(TAB_MODULES.lookups),
  moderation: lazy(TAB_MODULES.moderation),
  starfish: lazy(TAB_MODULES.starfish),
  raw: lazy(TAB_MODULES.raw),
};

export function AccountPage() {
  const { discordId = "" } = useParams();
  const core = useQuery(accountCoreQuery(discordId));
  return (
    <QueryContent query={core} skeleton={<SkeletonLines widths={["30%", "60%", "45%", "70%"]} />}>
      {(data) => <AccountLayout core={data} />}
    </QueryContent>
  );
}

function AccountLayout({ core }: { core: AccountCore }) {
  const [tab, setTab] = useUrlEnum("tab", ACCOUNT_TABS, DEFAULT_TAB, { history: "push" });
  const Panel = TAB_PANELS[tab];
  useEffect(() => {
    for (const load of Object.values(TAB_MODULES)) void load();
  }, []);
  return (
    <>
      <AccountHeader core={core} />
      <div className="grid grid-cols-1 items-start gap-5 lg:grid-cols-[300px_minmax(0,1fr)] xl:grid-cols-[340px_minmax(0,1fr)]">
        <AccountRail core={core} />
        <div className="flex min-w-0 flex-col gap-4">
          <Tabs label="Account sections" value={tab} onChange={setTab} idPrefix="account" tabs={ACCOUNT_TABS.map((value) => ({ value, label: TAB_LABEL[value], count: tabCount(core, value) }))} />
          <div id="account-panel" role="tabpanel" aria-labelledby={`account-tab-${tab}`} className="flex min-w-0 flex-col gap-4">
            <Suspense fallback={<SkeletonLines />}>
              <Panel core={core} />
            </Suspense>
          </div>
        </div>
      </div>
    </>
  );
}

function tabCount(core: AccountCore, tab: AccountTab): number | null {
  if (tab === "network") return core.counts.ips;
  return null;
}
