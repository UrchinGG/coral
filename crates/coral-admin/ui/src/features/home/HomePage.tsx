import { AttentionFeed } from "./AttentionFeed";
import { ModerationPanels } from "./ModerationPanels";
import { PulseStrip } from "./PulseStrip";
import { RecentActions } from "./RecentActions";
import { RecentlyViewed } from "./RecentlyViewed";

export function HomePage() {
  return (
    <>
      <div className="flex flex-col gap-1">
        <h1 className="text-22 font-semibold">Home</h1>
        <p className="m-0 text-muted">Last 60 minutes. The pulse refreshes every 15 s, flags and admin actions every 30 s, moderation every minute.</p>
      </div>
      <PulseStrip />
      <AttentionFeed />
      <div className="grid gap-4 xl:grid-cols-3">
        <RecentActions />
        <ModerationPanels />
      </div>
      <RecentlyViewed />
    </>
  );
}
