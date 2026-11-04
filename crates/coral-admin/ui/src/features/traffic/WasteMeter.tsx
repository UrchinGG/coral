import type { WasteUsage } from "../../api/types/traffic";
import { Pill } from "../../kit/Badges";
import { Meter } from "../../kit/Meter";
import { useTimeFormat } from "../../lib/time";

export function WasteMeter({ usage, allowance }: { usage: WasteUsage; allowance: number }) {
  const { time } = useTimeFormat();
  if (usage.blocked_until !== null) {
    return <Pill tone="danger">blocked until {time(usage.blocked_until, { seconds: false, zone: true })}</Pill>;
  }
  return <Meter used={Math.round(usage.spent)} limit={allowance} label="wasted calls on offline players" />;
}
