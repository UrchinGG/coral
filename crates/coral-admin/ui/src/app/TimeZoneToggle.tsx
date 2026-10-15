import { Segmented } from "../kit/Segmented";
import { setTimeZoneMode, useTimeZoneMode, type TimeZoneMode } from "../lib/time";

const OPTIONS = [
  { value: "utc", label: "UTC" },
  { value: "local", label: "Local" },
] as const satisfies readonly { value: TimeZoneMode; label: string }[];

export function TimeZoneToggle() {
  const mode = useTimeZoneMode();
  return <Segmented label="Time zone" value={mode} options={OPTIONS} onChange={setTimeZoneMode} className="max-md:hidden" />;
}
