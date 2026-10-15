import type { LimitPressure, Limiter, LimitUsage } from "../../api/types/traffic";
import { EmptyState } from "../../kit/States";
import { EntityLink } from "../../kit/EntityLink";
import { Meter } from "../../kit/Meter";
import { formatNumber, formatPercent } from "../../lib/format";
import { refIdentity } from "../../lib/refs";

const SHOWN_LIMITS = 2;
const SECONDS_PER_MINUTE = 60;

const LIMITER_LABEL: Record<Limiter, string> = {
  personal: "personal key",
  developer: "developer key",
  session: "Starfish session",
  session_uuids: "Starfish batch players",
  key_uuids: "distinct players",
  tagging: "tagging",
};

export function PressurePanel({ pressure }: { pressure: LimitPressure }) {
  const minutes = pressure.window_s / SECONDS_PER_MINUTE;
  if (pressure.accounts.length === 0) {
    return (
      <EmptyState>
        No account is above {formatPercent(pressure.threshold, 0)} of a limit. Checked {formatNumber(pressure.candidates)} callers seen in the last {minutes} minutes.
      </EmptyState>
    );
  }
  return (
    <ul className="m-0 flex list-none flex-col p-0">
      {pressure.accounts.map((account) => (
        <li
          key={refIdentity(account.account)}
          className="grid grid-cols-[minmax(110px,170px)_minmax(0,1fr)] items-center gap-x-3.5 gap-y-1.5 border-b border-border px-3.5 py-2.5 last:border-b-0 max-md:grid-cols-1"
        >
          <EntityLink entity={account.account} tab="activity" />
          <div className="flex flex-col gap-2">
            {account.limits.slice(0, SHOWN_LIMITS).map((limit) => (
              <LimitMeter key={limit.limiter} limit={limit} minutes={minutes} />
            ))}
          </div>
        </li>
      ))}
    </ul>
  );
}

function LimitMeter({ limit, minutes }: { limit: LimitUsage; minutes: number }) {
  return <Meter used={limit.used} limit={limit.limit} label={`${LIMITER_LABEL[limit.limiter]} / ${minutes} min`} />;
}
