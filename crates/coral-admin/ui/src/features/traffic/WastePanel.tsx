import type { WasteOverview } from "../../api/types/traffic";
import { EntityLink } from "../../kit/EntityLink";
import { EmptyState } from "../../kit/States";
import { refIdentity } from "../../lib/refs";
import { WasteMeter } from "./WasteMeter";

export function WastePanel({ overview }: { overview: WasteOverview }) {
  if (overview.keys.length === 0) {
    return <EmptyState>No personal key is spending its allowance or blocked.</EmptyState>;
  }
  return (
    <ul className="m-0 flex list-none flex-col p-0">
      {overview.keys.map((key) => (
        <li
          key={refIdentity(key.account)}
          className="grid grid-cols-[minmax(110px,170px)_minmax(0,1fr)] items-center gap-x-3.5 gap-y-1.5 border-b border-border px-3.5 py-2.5 last:border-b-0 max-md:grid-cols-1"
        >
          <EntityLink entity={key.account} tab="activity" />
          <WasteMeter usage={key} allowance={overview.allowance} />
        </li>
      ))}
    </ul>
  );
}
