import type { ReactNode } from "react";

export type KeyValueItem = { key?: string; label: ReactNode; value: ReactNode };

export function KeyValue({ items }: { items: readonly KeyValueItem[] }) {
  return (
    <dl className="grid grid-cols-[minmax(96px,128px)_minmax(0,1fr)] items-baseline gap-x-3 gap-y-[7px] text-12 max-md:grid-cols-[minmax(84px,104px)_minmax(0,1fr)]">
      {items.map((item, index) => (
        <div key={item.key ?? index} className="contents">
          <dt className="text-subtle">{item.label}</dt>
          <dd className="m-0 min-w-0 wrap-anywhere">{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}
