import { Tabs } from "../../kit/Tabs";
import { useUrlEnum } from "../../lib/url-state";
import { PageHead } from "../platform/PageHead";
import { AuditTab } from "./AuditTab";
import { BlacklistTab } from "./BlacklistTab";
import { GuideTab } from "./GuideTab";
import { MODERATION_TABS } from "./params";
import { StrikesTab } from "./StrikesTab";

const TAB_ITEMS = [
  { value: "blacklist", label: "Blacklist" },
  { value: "strikes", label: "Strikes" },
  { value: "audit", label: "Audit log" },
  { value: "guide", label: "Guide and rules" },
] as const;

export function ModerationPage() {
  const [tab, setTab] = useUrlEnum("tab", MODERATION_TABS, "blacklist", { history: "push" });
  return (
    <>
      <PageHead title="Moderation" lede="Active tags, account strikes, every admin action, and the tag review guide with its rules." />
      <Tabs label="Moderation" value={tab} tabs={TAB_ITEMS} onChange={setTab} idPrefix="moderation" />
      <div id="moderation-panel" role="tabpanel" aria-labelledby={`moderation-tab-${tab}`} className="flex flex-col gap-4">
        {tab === "blacklist" && <BlacklistTab />}
        {tab === "strikes" && <StrikesTab />}
        {tab === "audit" && <AuditTab />}
        {tab === "guide" && <GuideTab />}
      </div>
    </>
  );
}
